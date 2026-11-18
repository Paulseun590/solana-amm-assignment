use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{transfer, Mint, Token, TokenAccount, Transfer},
};
use constant_product_curve::{ConstantProduct, LiquidityPair};

use crate::{error::AmmError, state::Config};

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    pub mint_x: Box<Account<'info, Mint>>,
    pub mint_y: Box<Account<'info, Mint>>,

    #[account(
        has_one = mint_x,
        has_one = mint_y,
        seeds = [b"config", config.seed.to_le_bytes().as_ref()],
        bump = config.config_bump,
    )]
    pub config: Account<'info, Config>,

    #[account(
        seeds = [b"lp", config.key().as_ref()],
        bump = config.lp_bump,
    )]
    pub mint_lp: Box<Account<'info, Mint>>,

    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = config,
    )]
    pub vault_x: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = config,
    )]
    pub vault_y: Box<Account<'info, TokenAccount>>,

    /// CHECK: PDA is validated and used only as the treasury token-account authority.
    #[account(
        seeds = [b"treasury", config.key().as_ref()],
        bump,
    )]
    pub treasury_authority: UncheckedAccount<'info>,

    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = treasury_authority,
    )]
    pub treasury_x: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = treasury_authority,
    )]
    pub treasury_y: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = user,
    )]
    pub user_x: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = user,
    )]
    pub user_y: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> Swap<'info> {
    pub fn swap(&mut self, is_x: bool, amount: u64, min: u64) -> Result<()> {
        require!(amount > 0, AmmError::InvalidAmount);
        require!(!self.config.locked, AmmError::PoolLocked);

        // Calculate the fee in basis points.
        let fee_multiplier = 10_000u64
            .checked_sub(self.config.fee as u64)
            .ok_or(AmmError::InvalidFee)?;

        let net_amount = (amount as u128)
            .checked_mul(fee_multiplier as u128)
            .ok_or(AmmError::Overflow)?
            .checked_div(10_000)
            .ok_or(AmmError::InvalidAmount)? as u64;

        let fee_amount = amount.checked_sub(net_amount).ok_or(AmmError::Underflow)?;

        require!(net_amount > 0, AmmError::InvalidAmount);

        // The curve receives only the net input. Its internal fee is zero
        // to avoid charging the user twice.
        let mut curve = ConstantProduct::init(
            self.vault_x.amount,
            self.vault_y.amount,
            self.mint_lp.supply,
            0,
            Some(6),
        )
        .map_err(AmmError::from)?;

        let pair = if is_x {
            LiquidityPair::X
        } else {
            LiquidityPair::Y
        };

        let swap_result = curve.swap(pair, net_amount, min).map_err(AmmError::from)?;

        // Transfer the net input into the pool.
        self.deposit_tokens(is_x, net_amount)?;

        // Transfer the fee into the treasury for the input token.
        if fee_amount > 0 {
            self.deposit_fee(is_x, fee_amount)?;
        }

        // Transfer the output token to the user.
        self.withdraw_tokens(is_x, swap_result.withdraw)
    }

    fn deposit_tokens(&self, is_x: bool, amount: u64) -> Result<()> {
        let (from, to) = if is_x {
            (
                self.user_x.to_account_info(),
                self.vault_x.to_account_info(),
            )
        } else {
            (
                self.user_y.to_account_info(),
                self.vault_y.to_account_info(),
            )
        };

        transfer(
            CpiContext::new(
                self.token_program.key(),
                Transfer {
                    from,
                    to,
                    authority: self.user.to_account_info(),
                },
            ),
            amount,
        )
    }

    fn deposit_fee(&self, is_x: bool, amount: u64) -> Result<()> {
        let (from, to) = if is_x {
            (
                self.user_x.to_account_info(),
                self.treasury_x.to_account_info(),
            )
        } else {
            (
                self.user_y.to_account_info(),
                self.treasury_y.to_account_info(),
            )
        };

        transfer(
            CpiContext::new(
                self.token_program.key(),
                Transfer {
                    from,
                    to,
                    authority: self.user.to_account_info(),
                },
            ),
            amount,
        )
    }

    fn withdraw_tokens(&self, is_x: bool, amount: u64) -> Result<()> {
        let (from, to) = if is_x {
            (
                self.vault_y.to_account_info(),
                self.user_y.to_account_info(),
            )
        } else {
            (
                self.vault_x.to_account_info(),
                self.user_x.to_account_info(),
            )
        };

        transfer(
            CpiContext::new_with_signer(
                self.token_program.key(),
                Transfer {
                    from,
                    to,
                    authority: self.config.to_account_info(),
                },
                &[&[
                    b"config",
                    &self.config.seed.to_le_bytes(),
                    &[self.config.config_bump],
                ]],
            ),
            amount,
        )
    }
}
