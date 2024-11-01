multiversx_sc::imports!();
multiversx_sc::derive_imports!();
#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, TypeAbi, PartialEq, Clone, ManagedVecItem)]
pub enum BetStatus {
    Matched,
    Unmatched,
    PartiallyMatched,
    Win,
    Lost,
    Canceled,
}

#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, TypeAbi, PartialEq, Clone, ManagedVecItem, Copy)]
pub enum BetType {
    Back,
    Lay
}

#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, TypeAbi, Clone, PartialEq)]
pub enum MarketStatus {
    Open,    
    Closed, 
    Settled
}

#[derive(TypeAbi, TopEncode, TopDecode, NestedEncode, NestedDecode, Clone, ManagedVecItem)]
pub struct Bet<M: ManagedTypeApi> {
    pub bettor: ManagedAddress<M>,
    pub event: u64, 
    pub selection: Selection<M>, 
    pub stake_amount: BigUint<M>, 
    pub liability: BigUint<M>, 
    pub matched_amount: BigUint<M>, 
    pub unmatched_amount: BigUint<M>, 
    pub potential_profit: BigUint<M>, 
    pub odd: BigUint<M>, 
    pub bet_type: BetType, 
    pub status: BetStatus, 
    pub payment_token: EgldOrEsdtTokenIdentifier<M>,
    pub payment_nonce: u64,
    pub nft_nonce: u64,
    pub created_at: u64, 
}

#[derive(TypeAbi, TopEncode, TopDecode, NestedEncode, NestedDecode, Clone)]
pub struct BetAttributes<M:ManagedTypeApi>{
    pub bettor: ManagedAddress<M>,
    pub event: u64,     
    pub selection: Selection<M>,     
    pub stake_amount: BigUint<M>, 
    pub liability: BigUint<M>,  
    pub matched_amount: BigUint<M>, 
    pub unmatched_amount: BigUint<M>,    
    pub potential_profit: BigUint<M>,     
    pub odd: BigUint<M>,        
    pub bet_type: BetType,      
    pub status: BetStatus,         
    pub payment_token: EgldOrEsdtTokenIdentifier<M>, 
    pub payment_nonce: u64,
    pub created_at: u64, 
}

#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, TypeAbi, Clone)]
pub struct Market<M: ManagedTypeApi> {
    pub market_id: u64,
    pub event_id: u64,
    pub description: ManagedBuffer<M>,
    pub selections: ManagedVec<M, Selection<M>>,
    pub close_timestamp: u64,
    pub market_status: MarketStatus,
    pub total_matched_amount: BigUint<M>,
    pub liquidity: BigUint<M>,
    pub created_at: u64,
}

#[derive(TypeAbi, TopEncode, TopDecode, NestedEncode, NestedDecode, Clone, ManagedVecItem)]
pub struct Selection<M: ManagedTypeApi> {
    pub selection_id: u64,
    pub description: ManagedBuffer<M>,
    pub priority_queue: Tracker<M>,
}

#[derive(TypeAbi, TopEncode, TopDecode, NestedEncode, NestedDecode, Clone, ManagedVecItem)]
pub struct PriceLevel<M: ManagedTypeApi> {
    pub odds: BigUint<M>,
    pub total_stake: BigUint<M>,
    pub bet_nonces: ManagedVec<M, u64>,
}

#[derive(TypeAbi, TopEncode, TopDecode, NestedEncode, NestedDecode, Clone, ManagedVecItem)]
pub struct Tracker<M: ManagedTypeApi> {
    pub back_levels: ManagedVec<M, PriceLevel<M>>,
    pub lay_levels: ManagedVec<M, PriceLevel<M>>,
    pub back_liquidity: BigUint<M>,
    pub lay_liquidity: BigUint<M>,
    pub matched_count: u64,
    pub unmatched_count: u64,
    pub partially_matched_count: u64,
    pub win_count: u64,
    pub lost_count: u64,
    pub canceled_count: u64,
}

//Debugging
#[derive(TypeAbi, TopEncode, TopDecode, NestedEncode, NestedDecode, ManagedVecItem)]
pub struct BetMatchingState<M: ManagedTypeApi> {
    pub bet_type: BetType,
    pub original_stake: BigUint<M>,
    pub matched_amount: BigUint<M>,
    pub unmatched_amount: BigUint<M>,
    pub status: BetStatus,
    pub odds: BigUint<M>
}

#[derive(TypeAbi, TopEncode, TopDecode, NestedEncode, NestedDecode, ManagedVecItem)]
pub struct MatchingDetails<M: ManagedTypeApi> {
    pub back_levels: ManagedVec<M,PriceLevelView<M>>,
    pub lay_levels: ManagedVec<M,PriceLevelView<M>>,
    pub back_liquidity: BigUint<M>,
    pub lay_liquidity: BigUint<M>,
    pub matched_count: u64,
    pub unmatched_count: u64,
    pub partially_matched_count: u64
}

#[derive(TypeAbi, TopEncode, TopDecode, NestedEncode, NestedDecode, ManagedVecItem)]
pub struct PriceLevelView<M: ManagedTypeApi> {
    pub odds: BigUint<M>,
    pub total_stake: BigUint<M>,
    pub bets: ManagedVec<M, BetView<M>>
}

#[derive(TypeAbi, TopEncode, TopDecode, NestedEncode, NestedDecode, ManagedVecItem)]
pub struct BetView<M: ManagedTypeApi> {
    pub nonce: u64,
    pub bettor: ManagedAddress<M>,
    pub stake: BigUint<M>,
    pub matched: BigUint<M>,
    pub unmatched: BigUint<M>,
    pub status: BetStatus
}

#[derive(TypeAbi, TopEncode, TopDecode, ManagedVecItem, Debug)]
pub struct OrderbookView<M: ManagedTypeApi> {
    pub price_level: BigUint<M>,
    pub total_amount: BigUint<M>,
    pub bet_count: u32
}



