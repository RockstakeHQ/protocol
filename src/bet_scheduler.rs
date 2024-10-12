multiversx_sc::imports!();
multiversx_sc::derive_imports!();

use crate::types::{Bet, BetStatus, BetType};

#[derive(TypeAbi, TopEncode, TopDecode, NestedEncode, NestedDecode, Clone, ManagedVecItem)]
pub struct BetScheduler<M: ManagedTypeApi> {
    back_bets: ManagedVec<M, Bet<M>>,
    lay_bets: ManagedVec<M, Bet<M>>,
    best_back_odds: BigUint<M>,
    best_lay_odds: BigUint<M>,
    back_liquidity: BigUint<M>,
    lay_liquidity: BigUint<M>,
}

impl<M: ManagedTypeApi> BetScheduler<M> {
    pub fn new() -> Self {
        BetScheduler {
            back_bets: ManagedVec::new(),
            lay_bets: ManagedVec::new(),
            best_back_odds: BigUint::zero(),
            best_lay_odds: BigUint::zero(),
            back_liquidity: BigUint::zero(),
            lay_liquidity: BigUint::zero(),
        }
    }

    pub fn add(&mut self, mut bet: Bet<M>) {
        bet.status = BetStatus::Unmatched;
        match bet.bet_type {
            BetType::Back => {
                self.insert_bet(&mut self.back_bets.clone(), bet.clone());
                self.back_liquidity += &bet.stake_amount;
                self.update_best_back_odds();
            },
            BetType::Lay => {
                self.insert_bet(&mut self.lay_bets.clone(), bet.clone());
                self.lay_liquidity += &bet.liability;
                self.update_best_lay_odds();
            },
        };
    }

    fn insert_bet(&mut self, queue: &mut ManagedVec<M, Bet<M>>, bet: Bet<M>) {
        let mut insert_index = queue.len();
        for i in 0..queue.len() {
            if self.should_insert_before(&bet, &queue.get(i), bet.bet_type == BetType::Back) {
                insert_index = i;
                break;
            }
        }
        
        // Create a new vector with the bet inserted at the correct position
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

    fn should_insert_before(&self, new_bet: &Bet<M>, existing_bet: &Bet<M>, is_back: bool) -> bool {
        if is_back {
            new_bet.odd > existing_bet.odd || 
            (new_bet.odd == existing_bet.odd && new_bet.created_at < existing_bet.created_at)
        } else {
            new_bet.odd < existing_bet.odd || 
            (new_bet.odd == existing_bet.odd && new_bet.created_at < existing_bet.created_at)
        }
    }

    pub fn remove(&mut self, bet: &Bet<M>) -> Option<Bet<M>> {
        let queue = match bet.bet_type {
            BetType::Back => &mut self.back_bets,
            BetType::Lay => &mut self.lay_bets,
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
            
            // Create a new vector without the removed bet
            let mut new_queue = ManagedVec::new();
            for i in 0..queue.len() {
                if i != index {
                    new_queue.push(queue.get(i));
                }
            }
            *queue = new_queue;

            match bet.bet_type {
                BetType::Back => {
                    self.back_liquidity -= &removed_bet.unmatched_amount;
                    self.update_best_back_odds();
                },
                BetType::Lay => {
                    self.lay_liquidity -= &removed_bet.liability;
                    self.update_best_lay_odds();
                },
            }
            Some(removed_bet)
        } else {
            None
        }
    }

    pub fn get_matching_bets(&mut self, bet: &Bet<M>) -> (ManagedVec<M, Bet<M>>, BigUint<M>, BigUint<M>) {
        let mut matched_amount = BigUint::zero();
        let mut unmatched_amount = bet.stake_amount.clone();
        let mut matching_bets = ManagedVec::new();
    
        let source = match bet.bet_type {
            BetType::Back => &mut self.lay_bets,
            BetType::Lay => &mut self.back_bets,
        };
    
        for i in 0..source.len() {
            let existing_bet = source.get(i);
            let is_match = match bet.bet_type {
                BetType::Back => bet.odd >= existing_bet.odd,
                BetType::Lay => bet.odd <= existing_bet.odd,
            };
    
            if is_match {
                let match_amount = unmatched_amount.clone().min(existing_bet.unmatched_amount.clone());
    
                matched_amount += &match_amount;
                unmatched_amount -= &match_amount;
    
                let mut updated_bet = existing_bet.clone();
                updated_bet.matched_amount += &match_amount;
                updated_bet.unmatched_amount -= &match_amount;
                matching_bets.push(updated_bet);
    
                if unmatched_amount == BigUint::zero() {
                    break;
                }
            } else {
                break;  // No more matching bets due to ordering
            }
        }
        (matching_bets, matched_amount, unmatched_amount)
    }

    pub fn match_bet(&mut self, bet: &mut Bet<M>) -> (BigUint<M>, BigUint<M>) {
        let (matching_bets, matched_amount, unmatched_amount) = self.get_matching_bets(bet);
        
        // Actualizăm statusul pariului curent
        bet.matched_amount = matched_amount.clone();
        bet.unmatched_amount = unmatched_amount.clone();
        bet.status = if matched_amount == bet.stake_amount {
            BetStatus::Matched
        } else if matched_amount > BigUint::zero() {
            BetStatus::PartiallyMatched
        } else {
            BetStatus::Unmatched
        };

        // Actualizăm pariurile potrivite și le readăugăm dacă este necesar
        for mut matched_bet in matching_bets.iter() {
            self.remove(&matched_bet);
            
            if matched_bet.matched_amount == matched_bet.stake_amount {
                matched_bet.status = BetStatus::Matched;
            } else {
                matched_bet.status = BetStatus::PartiallyMatched;
                self.add(matched_bet);
            }
        }

        // Adăugăm pariul curent dacă nu este complet potrivit
        if bet.unmatched_amount > BigUint::zero() {
            self.add(bet.clone());
        }

        (matched_amount, unmatched_amount)
    }


    fn update_best_back_odds(&mut self) {
        if self.back_bets.is_empty() {
            self.best_back_odds = BigUint::zero();
        } else {
            self.best_back_odds = self.back_bets.get(0).odd.clone();
        }
    }

    fn update_best_lay_odds(&mut self) {
        if self.lay_bets.is_empty() {
            self.best_lay_odds = BigUint::zero();
        } else {
            self.best_lay_odds = self.lay_bets.get(0).odd.clone();
        }
    }

    pub fn get_back_bets(&self) -> ManagedVec<M, Bet<M>>{
        self.back_bets.clone()
    }

    pub fn get_lay_bets(&self) -> ManagedVec<M, Bet<M>>{
        self.lay_bets.clone()
    }

    pub fn get_best_back_odds(&self) -> BigUint<M> {
        self.best_back_odds.clone()
    }

    pub fn get_best_lay_odds(&self) -> BigUint<M> {
        self.best_lay_odds.clone()
    }

    pub fn get_back_liquidity(&self) -> BigUint<M> {
        self.back_liquidity.clone()
    }

    pub fn get_lay_liquidity(&self) -> BigUint<M> {
        self.lay_liquidity.clone()
    }

    pub fn get_top_n_bets(&self, bet_type: BetType, n: usize) -> ManagedVec<M, Bet<M>> {
        let source = match bet_type {
            BetType::Back => &self.back_bets,
            BetType::Lay => &self.lay_bets,
        };
        let mut result = ManagedVec::new();
        for i in 0..n.min(source.len()) {
            result.push(source.get(i).clone());
        }
        result
    }

    pub fn get_total_bets(&self) -> usize {
        self.back_bets.len() + self.lay_bets.len()
    }
}