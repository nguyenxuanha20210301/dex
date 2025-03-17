use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct InstantiateMsg {
    // pub owner: String, 
    pub usdt_contract: String, 
    pub lpt_contract: String, 
}

#[cw_serde]
pub enum ExecuteMsg {
    AddLiquidity { orai_amount: Uint128, usdt_amount: Uint128 },
    RemoveLiquidity { lpt_amount: Uint128 },
    Swap { denom: String, amount: Uint128 },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ContractInfoResponse)]
    QueryContractInfo {},

    #[returns(PoolInfoResponse)]
    QueryPoolInfo {},

    #[returns(LptBalanceResponse)]
    QueryLptBalance { user: String },

}


#[cw_serde]

pub struct PoolInfoResponse {
    pub orai_reserve: Uint128, 
    pub usdt_reserve: Uint128, 
    pub total_shares: Uint128, 
}


#[cw_serde]
pub struct LptBalanceResponse {
    pub balance: Uint128, 
}


#[cw_serde]
pub struct ContractInfoResponse {
    pub owner: String, 
    pub lpt_contract: String, 
    pub usdt_contract: String, 
}