use multiversx_sc::codec::multi_types::MultiValue6;
multiversx_sc::imports!();
multiversx_sc::derive_imports!();

use crate::types::{Bet, BetScheduler, BetStatus, BetType};

#[multiversx_sc::module]
pub trait BetSchedulerModule:
    crate::storage::StorageModule +
    crate::events::EventsModule 
{
    // View Functions
    #[view(getSchedulerState)]
    fn get_scheduler_state(&self, market_id: u64, selection_id: u64) -> BetScheduler<Self::Api> {
        let market = self.markets(&market_id).get();
        let selection = market
            .selections
            .iter()
            .find(|s| s.selection_id == selection_id)
            .expect("Selection not found");
        
        selection.priority_queue.clone()
    }

    #[view(getBetCounts)]
    fn get_bet_counts(&self, market_id: u64, selection_id: u64) -> MultiValue6<BigUint, BigUint, BigUint, BigUint, BigUint, BigUint> {
        let scheduler = self.get_scheduler_state(market_id, selection_id);
        (
            BigUint::from(scheduler.matched_count),
            BigUint::from(scheduler.unmatched_count),
            BigUint::from(scheduler.partially_matched_count),
            BigUint::from(scheduler.win_count),
            BigUint::from(scheduler.lost_count),
            BigUint::from(scheduler.canceled_count)
        ).into()
    }

    #[view(getMarketLiquidity)]
    fn get_market_liquidity(&self, market_id: u64, selection_id: u64) -> MultiValue2<BigUint, BigUint> {
        let scheduler = self.get_scheduler_state(market_id, selection_id);
        (scheduler.back_liquidity, scheduler.lay_liquidity).into()
    }

    // Public Endpoints
    #[endpoint(updateBetStatus)]
    fn update_bet_status(
        &self,
        market_id: u64,
        selection_id: u64,
        bet_id: u64,
        new_status: BetStatus
    ) -> SCResult<()> {
        let mut market = self.markets(&market_id).get();
        let selection_index = market
            .selections
            .iter()
            .position(|s| s.selection_id == selection_id)
            .ok_or("Selection not found")?;

        let mut selection = market.selections.get(selection_index);
        let mut scheduler = selection.priority_queue.clone();
        
        let bet = self.bet_by_id(bet_id).get();
        let old_status = bet.status.clone();
        
        self.update_status_counters(&mut scheduler, &old_status, &new_status);
        
        selection.priority_queue = scheduler;
        let _ = market.selections.set(selection_index, &selection);
        self.markets(&market_id).set(&market);

        self.bet_status_updated_event(
            market_id,
            selection_id,
            bet_id,
            &old_status,
            &new_status
        );

        Ok(())
    }

    #[endpoint(matchBet)]
    fn match_bet(&self, bet: Bet<Self::Api>) -> SCResult<(BigUint, BigUint, Bet<Self::Api>)> {
        let mut scheduler = self.bet_scheduler().get();
        let old_status = bet.status.clone();
        let (matching_bets, matched_amount, unmatched_amount) = self.get_matching_bets(bet.clone());
        
        let mut updated_bet = bet;
        updated_bet.matched_amount = matched_amount.clone();
        updated_bet.unmatched_amount = unmatched_amount.clone();
        
        let new_status = self.calculate_bet_status(&updated_bet, &matched_amount);
    
        if old_status != new_status {
            self.update_status_counters(&mut scheduler, &old_status, &new_status);
        }
        updated_bet.status = new_status;
    
        self.process_matching_bets(&mut scheduler, matching_bets);
    
        if updated_bet.unmatched_amount > BigUint::zero() {
            self.add(updated_bet.clone());
        }
    
        self.bet_scheduler().set(scheduler);
        Ok((matched_amount, unmatched_amount, updated_bet))
    }

    // Internal Functions
    fn init_bet_scheduler(&self) -> BetScheduler<Self::Api> {
        BetScheduler {
            back_bets: ManagedVec::new(),
            lay_bets: ManagedVec::new(),
            best_back_odds: BigUint::zero(),
            best_lay_odds: BigUint::zero(),
            back_liquidity: BigUint::zero(),
            lay_liquidity: BigUint::zero(),
            matched_count: 0,
            unmatched_count: 0,
            partially_matched_count: 0,
            win_count: 0,
            lost_count: 0,
            canceled_count: 0,
        }
    }

    fn add(&self, bet: Bet<Self::Api>) {
        let mut scheduler = self.bet_scheduler().get();
        let old_status = bet.status.clone();
        let mut new_bet = bet;
        new_bet.status = BetStatus::Unmatched;
        self.update_status_counters(&mut scheduler, &old_status, &new_bet.status);

        match new_bet.bet_type {
            BetType::Back => {
                let mut queue = scheduler.back_bets.clone();
                self.insert_bet(&mut queue, new_bet.clone());
                scheduler.back_bets = queue;
                scheduler.back_liquidity += &new_bet.stake_amount;
                self.update_best_back_odds(&mut scheduler);
            },
            BetType::Lay => {
                let mut queue = scheduler.lay_bets.clone();
                self.insert_bet(&mut queue, new_bet.clone());
                scheduler.lay_bets = queue;
                scheduler.lay_liquidity += &new_bet.liability;
                self.update_best_lay_odds(&mut scheduler);
            },
        };
        self.bet_scheduler().set(scheduler);
    }

    fn remove(&self, bet: Bet<Self::Api>) -> Option<Bet<Self::Api>> {
        let mut scheduler = self.bet_scheduler().get();
        let queue = match bet.bet_type {
            BetType::Back => &mut scheduler.back_bets,
            BetType::Lay => &mut scheduler.lay_bets,
        };

        let mut index_to_remove = None;
        for i in 0..queue.len() {
            if queue.get(i).nft_nonce == bet.nft_nonce {
                index_to_remove = Some(i);
                break;
            }
        }

        if let Some(index) = index_to_remove {
            let removed_bet = queue.get(index);
            
            let mut new_queue = ManagedVec::new();
            for i in 0..queue.len() {
                if i != index {
                    new_queue.push(queue.get(i));
                }
            }
            *queue = new_queue;

            match bet.bet_type {
                BetType::Back => {
                    scheduler.back_liquidity -= &removed_bet.unmatched_amount;
                    self.update_best_back_odds(&mut scheduler);
                },
                BetType::Lay => {
                    scheduler.lay_liquidity -= &removed_bet.liability;
                    self.update_best_lay_odds(&mut scheduler);
                },
            }
            self.bet_scheduler().set(scheduler);
            Some(removed_bet)
        } else {
            None
        }
    }

    fn get_matching_bets(
        &self,
        bet: Bet<Self::Api>
    ) -> (ManagedVec<Self::Api, Bet<Self::Api>>, BigUint, BigUint) {
        let scheduler = self.bet_scheduler().get();
        let mut matched_amount = BigUint::zero();
        let mut unmatched_amount = match bet.bet_type {
            BetType::Back => bet.stake_amount.clone(),
            BetType::Lay => bet.liability.clone(),
        };
        let mut matching_bets = ManagedVec::new();
        let source = match bet.bet_type {
            BetType::Back => &scheduler.lay_bets,
            BetType::Lay => &scheduler.back_bets,
        };
    
        for i in 0..source.len() {
            let existing_bet = source.get(i);
            let is_match = self.is_matching_bet(&bet, &existing_bet);
    
            if is_match {
                let match_amount = self.calculate_match_amount(&bet, &existing_bet, &unmatched_amount);
    
                matched_amount += &match_amount;
                unmatched_amount -= &match_amount;
    
                let mut updated_bet = existing_bet.clone();
                self.update_matched_bet(&mut updated_bet, &match_amount, &bet);
                matching_bets.push(updated_bet);
    
                if unmatched_amount == BigUint::zero() {
                    break;
                }
            } else {
                break;
            }
        }
    
        (matching_bets, matched_amount, unmatched_amount)
    }

    fn insert_bet(&self, queue: &mut ManagedVec<Self::Api, Bet<Self::Api>>, bet: Bet<Self::Api>) {
        let mut insert_index = queue.len();
        for i in 0..queue.len() {
            if self.should_insert_before(&bet, &queue.get(i), bet.bet_type == BetType::Back) {
                insert_index = i;
                break;
            }
        }
        
        let mut new_queue = ManagedVec::new();
        for i in 0..insert_index {
            new_queue.push(queue.get(i));
        }
        new_queue.push(bet);
        for i in insert_index..queue.len() {
            new_queue.push(queue.get(i));
        }
        *queue = new_queue;
    }

    // Helper Functions
    fn should_insert_before(
        &self,
        new_bet: &Bet<Self::Api>,
        existing_bet: &Bet<Self::Api>,
        is_back: bool
    ) -> bool {
        if is_back {
            new_bet.odd > existing_bet.odd || 
            (new_bet.odd == existing_bet.odd && new_bet.created_at < existing_bet.created_at)
        } else {
            new_bet.odd < existing_bet.odd || 
            (new_bet.odd == existing_bet.odd && new_bet.created_at < existing_bet.created_at)
        }
    }

    fn is_matching_bet(&self, bet: &Bet<Self::Api>, existing_bet: &Bet<Self::Api>) -> bool {
        match bet.bet_type {
            BetType::Back => bet.odd >= existing_bet.odd,
            BetType::Lay => bet.odd <= existing_bet.odd,
        }
    }

    fn calculate_match_amount(
        &self,
        bet: &Bet<Self::Api>,
        existing_bet: &Bet<Self::Api>,
        unmatched_amount: &BigUint,
    ) -> BigUint {
        if bet.bet_type == BetType::Back {
            unmatched_amount.clone().min(existing_bet.unmatched_amount.clone())
        } else {
            unmatched_amount.clone().min(existing_bet.stake_amount.clone())
        }
    }

    fn calculate_bet_status(
        &self,
        bet: &Bet<Self::Api>,
        matched_amount: &BigUint
    ) -> BetStatus {
        match bet.bet_type {
            BetType::Back => {
                if matched_amount == &bet.stake_amount {
                    BetStatus::Matched
                } else if matched_amount > &BigUint::zero() {
                    BetStatus::PartiallyMatched
                } else {
                    BetStatus::Unmatched
                }
            },
            BetType::Lay => {
                if matched_amount == &bet.liability {
                    BetStatus::Matched
                } else if matched_amount > &BigUint::zero() {
                    BetStatus::PartiallyMatched
                } else {
                    BetStatus::Unmatched
                }
            }
        }
    }

    fn update_matched_bet(
        &self,
        bet: &mut Bet<Self::Api>,
        match_amount: &BigUint,
        matching_bet: &Bet<Self::Api>
    ) {
        bet.matched_amount += match_amount;
        bet.unmatched_amount -= match_amount;
        
        if matching_bet.bet_type == BetType::Lay {
            bet.liability = match_amount * &(matching_bet.odd.clone() - BigUint::from(1u32));
        }
    }

    fn process_matching_bets(
        &self,
        scheduler: &mut BetScheduler<Self::Api>,
        matching_bets: ManagedVec<Self::Api, Bet<Self::Api>>
    ) {
        for matched_bet in matching_bets.iter() {
            let old_matched_status = matched_bet.status.clone();
            self.remove(matched_bet.clone());
            
            let mut updated_matched_bet = matched_bet;
            let new_matched_status = self.calculate_bet_status(
                &updated_matched_bet,
                &updated_matched_bet.matched_amount
            );
    
            if old_matched_status != new_matched_status {
                self.update_status_counters(scheduler, &old_matched_status, &new_matched_status);
            }
            updated_matched_bet.status = new_matched_status;
    
            if updated_matched_bet.unmatched_amount > BigUint::zero() {
                self.add(updated_matched_bet);
            }
        }
    }

    fn update_status_counters(
        &self,
        scheduler: &mut BetScheduler<Self::Api>,
        old_status: &BetStatus,
        new_status: &BetStatus
    ) {
        match old_status {
            BetStatus::Matched => scheduler.matched_count = scheduler.matched_count.saturating_sub(1),
            BetStatus::Unmatched => scheduler.unmatched_count = scheduler.unmatched_count.saturating_sub(1),
            BetStatus::PartiallyMatched => scheduler.partially_matched_count = scheduler.partially_matched_count.saturating_sub(1),
            BetStatus::Win => scheduler.win_count = scheduler.win_count.saturating_sub(1),
            BetStatus::Lost => scheduler.lost_count = scheduler.lost_count.saturating_sub(1),
            BetStatus::Canceled => scheduler.canceled_count = scheduler.canceled_count.saturating_sub(1),
        }

        match new_status {
            BetStatus::Matched => scheduler.matched_count += 1,
            BetStatus::Unmatched => scheduler.unmatched_count += 1,
            BetStatus::PartiallyMatched => scheduler.partially_matched_count += 1,
            BetStatus::Win => scheduler.win_count += 1,
            BetStatus::Lost => scheduler.lost_count += 1,
            BetStatus::Canceled => scheduler.canceled_count += 1,
        }

        self.bet_counter_update_event(
            old_status,
            new_status,
            scheduler.matched_count as usize,
            scheduler.unmatched_count as usize,
            scheduler.partially_matched_count as usize,
            scheduler.win_count as usize,
            scheduler.lost_count as usize,
            scheduler.canceled_count as usize,
        );
    }

    fn update_best_back_odds(&self, scheduler: &mut BetScheduler<Self::Api>) {
        if scheduler.back_bets.is_empty() {
            scheduler.best_back_odds = BigUint::zero();
        } else {
            scheduler.best_back_odds = scheduler.back_bets.get(0).odd.clone();
        }
    }

    fn update_best_lay_odds(&self, scheduler: &mut BetScheduler<Self::Api>) {
        if scheduler.lay_bets.is_empty() {
            scheduler.best_lay_odds = BigUint::zero();
        } else {
            scheduler.best_lay_odds = scheduler.lay_bets.get(0).odd.clone();
        }
    }

    // Events
    #[event("bet_status_updated")]
    fn bet_status_updated_event(
        &self,
        #[indexed] market_id: u64,
        #[indexed] selection_id: u64,
        #[indexed] bet_id: u64,
        #[indexed] old_status: &BetStatus,
        #[indexed] new_status: &BetStatus,
    );

    #[event("scheduler_state_updated")]
    fn scheduler_state_updated_event(
        &self,
        #[indexed] market_id: u64,
        #[indexed] selection_id: u64,
        #[indexed] back_liquidity: &BigUint,
        #[indexed] lay_liquidity: &BigUint,
        #[indexed] best_back_odds: &BigUint,
        #[indexed] best_lay_odds: &BigUint
    );

    #[event("bet_matched")]
    fn bet_matched_event(
        &self,
        #[indexed] market_id: u64,
        #[indexed] selection_id: u64,
        #[indexed] bet_id: u64,
        #[indexed] matched_amount: &BigUint,
        #[indexed] remaining_unmatched: &BigUint
    );

    #[view(getBetSchedulerCounts)]
    fn get_bet_scheduler_counts(&self, scheduler: &BetScheduler<Self::Api>) -> MultiValue6<BigUint, BigUint, BigUint, BigUint, BigUint, BigUint> {
        (
            BigUint::from(scheduler.matched_count),
            BigUint::from(scheduler.unmatched_count),
            BigUint::from(scheduler.partially_matched_count),
            BigUint::from(scheduler.win_count),
            BigUint::from(scheduler.lost_count),
            BigUint::from(scheduler.canceled_count)
        ).into()
    }
}