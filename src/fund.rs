use crate::{errors::{ERR_INVALID_MARKET, ERR_MARKET_NOT_CLOSED, ERR_MARKET_NOT_SETTLED}, types::{Bet, BetStatus, BetType, MarketStatus, MarketType, ProcessingProgress, ProcessingStatus}};
multiversx_sc::imports!();
multiversx_sc::derive_imports!();

#[multiversx_sc::module]
pub trait FundModule:
    crate::storage::StorageModule
    + crate::events::EventsModule
    + crate::nft::NftModule
{
    fn handle_expired_market(&self, market_id: u64) -> SCResult<()> {
        let mut market = self.markets(market_id).get();
        
        market.market_status = MarketStatus::Closed;
        self.markets(market_id).set(&market);
        self.process_unmatched_bets(market_id)?;
        self.market_closed_event(
            market_id,
            self.blockchain().get_block_timestamp()
        );

        Ok(())
    }

    fn process_unmatched_bets(&self, market_id: u64) -> SCResult<()> {
        let market = self.markets(market_id).get();
        
        for selection in market.selections.iter() {
            let back_levels = self.selection_back_levels(market_id, selection.id).get();
            for level in back_levels.iter() {
                for bet_nonce in level.bet_nonces.iter() {
                    self.process_unmatched_bet(bet_nonce)?;
                }
            }

            let lay_levels = self.selection_lay_levels(market_id, selection.id).get();
            for level in lay_levels.iter() {
                for bet_nonce in level.bet_nonces.iter() {
                    self.process_unmatched_bet(bet_nonce)?;
                }
            }

            self.selection_back_liquidity(market_id, selection.id).set(&BigUint::zero());
            self.selection_lay_liquidity(market_id, selection.id).set(&BigUint::zero());
        }

        Ok(())
    }

    fn process_unmatched_bet(&self, bet_nonce: u64) -> SCResult<()> {
        let mut bet = self.bet_by_id(bet_nonce).get();
        
        if bet.unmatched_amount > BigUint::zero() {
            let refund_amount = bet.unmatched_amount.clone();
            
            self.send().direct(
                &bet.bettor,
                &bet.payment_token,
                bet.payment_nonce,
                &refund_amount,
            );
            let original_matched = bet.matched_amount.clone();
            bet.unmatched_amount = BigUint::zero();
            bet.matched_amount = original_matched; 
            bet.status = if bet.matched_amount > BigUint::zero() {
                BetStatus::Matched
            } else {
                BetStatus::Canceled
            };
            self.bet_by_id(bet_nonce).set(&bet);
            self.bet_refunded_event(bet_nonce, &bet.bettor, &refund_amount);
        }
    
        Ok(())
    }

    #[only_owner]
    #[endpoint(setMarketResult)]
    fn set_market_result(
        &self,
        event_id: u64,
        market_type_id: u64,
        score_home: u32,
        score_away: u32
    ) -> SCResult<()> {
        let market_type = MarketType::from_u64(market_type_id)?;
        let market_id = self.get_market_id(event_id, market_type_id)?;
        let mut market = self.markets(market_id).get();
        
        require!(market.market_status == MarketStatus::Closed, ERR_MARKET_NOT_CLOSED);

        let winning_selection = self.determine_winner(market_type, score_home, score_away)?;
        self.winning_selection(market_id).set(winning_selection);

        self.current_processing_index(market_id).set(0u64);

        market.market_status = MarketStatus::Settled;
        self.markets(market_id).set(&market);

        Ok(())
    }

    #[endpoint(processBatchBets)]
    fn process_batch_bets(
        &self,
        market_id: u64,
        batch_size: u64
    ) -> SCResult<ProcessingStatus> {
        require!(
            self.markets(market_id).get().market_status == MarketStatus::Settled,
            ERR_MARKET_NOT_SETTLED
        );

        let winning_selection = self.winning_selection(market_id).get();
        let mut processed_count = 0u64;

        for bet_id in self.market_bet_ids(market_id).iter() {
            if processed_count >= batch_size {
                return Ok(ProcessingStatus::InProgress);
            }

            let mut bet = self.bet_by_id(bet_id).get();
            if bet.matched_amount > BigUint::zero() {
                match bet.bet_type {
                    BetType::Back => {
                        if bet.selection.id == winning_selection {
                            bet.status = BetStatus::Win;
                            let payout = &bet.matched_amount + &bet.potential_profit;
                            
                            self.send().direct(
                                &bet.bettor,
                                &bet.payment_token,
                                bet.payment_nonce,
                                &payout
                            );

                            self.reward_distributed_event(
                                bet.nft_nonce,
                                &bet.bettor,
                                &payout
                            );
                        } else {
                            bet.status = BetStatus::Lost;
                        }
                    },
                    BetType::Lay => {
                        if bet.selection.id != winning_selection {
                            bet.status = BetStatus::Win;
                            let payout = &bet.matched_amount + &bet.potential_profit;
                            
                            self.send().direct(
                                &bet.bettor,
                                &bet.payment_token,
                                bet.payment_nonce,
                                &payout
                            );

                            self.reward_distributed_event(
                                bet.nft_nonce,
                                &bet.bettor,
                                &payout
                            );
                        } else {
                            bet.status = BetStatus::Lost;
                        }
                    }
                }
                self.bet_by_id(bet_id).set(&bet);
                processed_count += 1;
            }
        }
        Ok(ProcessingStatus::Completed)
    }

    fn process_selection_bets(
        &self,
        market_id: u64,
        selection_id: u64,
        is_winning: bool,
        is_back: bool,
        max_to_process: u64
    ) -> SCResult<u64> {
        let levels = if is_back {
            self.selection_back_levels(market_id, selection_id).get()
        } else {
            self.selection_lay_levels(market_id, selection_id).get()
        };

        let mut processed_count = 0u64;

        for level in levels.iter() {
            for bet_nonce in level.bet_nonces.iter() {
                if processed_count >= max_to_process {
                    return Ok(processed_count);
                }

                let mut bet = self.bet_by_id(bet_nonce).get();
                if bet.matched_amount > BigUint::zero() {
                    let should_win = if is_back { is_winning } else { !is_winning };
                    
                    if should_win {
                        bet.status = BetStatus::Win;
                        
                        let payout = if is_back {
                            &bet.matched_amount + &bet.potential_profit
                        } else {
                            bet.matched_amount.clone()
                        };

                        self.send().direct(
                            &bet.bettor,
                            &bet.payment_token,
                            bet.payment_nonce,
                            &payout
                        );

                        self.reward_distributed_event(
                            bet.nft_nonce,
                            &bet.bettor,
                            &payout
                        );
                    } else {
                        bet.status = BetStatus::Lost;
                    }
                    
                    self.bet_by_id(bet_nonce).set(&bet);
                    processed_count += 1;
                }
            }
        }

        Ok(processed_count)
    }

    #[view(getWinningSelection)]
    fn get_winning_selection(&self, market_id: u64) -> u64 {
        self.winning_selection(market_id).get()
    }

    #[view(getMarketSettlementDetails)]
    fn get_market_settlement_details(
        &self,
        market_id: u64
    ) -> (u64, MarketStatus) {
        let market = self.markets(market_id).get();
        let winning_selection = if self.winning_selection(market_id).is_empty() {
            0u64
        } else {
            self.winning_selection(market_id).get()
        };
        
        (winning_selection, market.market_status)
    }

    #[view(getBetStatusDetails)]
    fn get_bet_status_details(
        &self,
        bet_nonce: u64
    ) -> (BetStatus, BigUint<Self::Api>, BigUint<Self::Api>) {
        let bet = self.bet_by_id(bet_nonce).get();
        (bet.status, bet.matched_amount, bet.potential_profit)
    }

    #[view(getProcessingProgress)]
    fn get_processing_progress(&self, market_id: u64) -> ProcessingProgress {
        let current_index = if self.current_processing_index(market_id).is_empty() {
            0u64
        } else {
            self.current_processing_index(market_id).get()
        };

        ProcessingProgress {
            market_id,
            processed_bets: current_index,
            status: if current_index > 0 { 
                ProcessingStatus::InProgress 
            } else { 
                ProcessingStatus::Completed 
            }
        }
    }

    #[inline]
    fn get_market_id(&self, event_id: u64, market_type_id: u64) -> SCResult<u64> {
        let markets = self.markets_by_event(event_id).get();
        require!(!markets.is_empty(), ERR_INVALID_MARKET);
        Ok(markets.get(market_type_id as usize - 1))
    }

    #[inline]
    fn determine_winner(
        &self,
        market_type: MarketType,
        score_home: u32,
        score_away: u32
    ) -> SCResult<u64> {
        match market_type {
            MarketType::FullTimeResult => {
                Ok(if score_home > score_away { 1u64 }
                   else if score_home < score_away { 2u64 }
                   else { 3u64 })
            },
            MarketType::TotalGoals => {
                Ok(if score_home + score_away > 2 { 1u64 }
                   else { 2u64 })
            },
            MarketType::BothTeamsToScore => {
                Ok(if score_home > 0 && score_away > 0 { 1u64 }
                   else { 2u64 })
            }
        }
    }
}