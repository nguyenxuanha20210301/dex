use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct LiquidityPool {
    pub orai_reserve: Uint128,  // ORAI native token
    pub usdt_reserve: Uint128,  // USDT CW20 token
    pub total_shares: Uint128,    // Total liquidity shares
}

#[cw_serde]
pub struct ContractInfo {
    pub owner: Addr, 
    pub usdt_contract: String, 
    pub lpt_contract: String, 
}
// Storage for the liquidity pool
pub const POOL: Item<LiquidityPool> = Item::new("pool");

// Mapping from user address to their liquidity shares
pub const LIQUIDITY_PROVIDERS: Map<&Addr, Uint128> = Map::new("liquidity_providers");

//storage for the contract info 
pub const INFO: Item<ContractInfo> = Item::new("contract_info");

//allowance amount usdt
pub const USDT_ALLOWANCE: Map<&Addr, Uint128> = Map::new("usdt_allowance");