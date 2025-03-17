#[cfg(not(feature = "library"))]
// use std::env;
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, BankMsg, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg};
// use cw2::set_contract_version;
use cw20::{AllowanceResponse, Cw20ExecuteMsg};
use crate::error::ContractError;
use crate::msg::{ExecuteMsg, ContractInfoResponse, LptBalanceResponse, PoolInfoResponse, InstantiateMsg, QueryMsg};
use crate::state::{ContractInfo, LiquidityPool, INFO, LIQUIDITY_PROVIDERS, POOL};

// version info for migration info
// const CONTRACT_NAME: &str = "crates.io:dex";
// const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const DENOM_ORAI: &str = "orai";
const DENOM_USDT: &str = "usdt";
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let contract_info = ContractInfo {
        owner: info.sender.clone(),
        usdt_contract: msg.usdt_contract.clone(), 
        lpt_contract: msg.lpt_contract.clone(),
    };

    let pool = LiquidityPool {
        orai_reserve: Uint128::zero(),
        usdt_reserve: Uint128::zero(),
        total_shares: Uint128::zero(),
    };

    // set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    INFO.save(deps.storage, &contract_info)?;
    POOL.save(deps.storage, &pool)?;

    Ok(Response::new()  
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender.clone())
        .add_attribute("usdt_contract", msg.usdt_contract)
        .add_attribute("lpt_contract", msg.lpt_contract))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    // unimplemented!()
    match msg {
        ExecuteMsg::AddLiquidity { orai_amount, usdt_amount } => execute::add_liquidity(deps, _env, info, orai_amount, usdt_amount),
        ExecuteMsg::RemoveLiquidity { lpt_amount } => execute::remove_liquidity(deps, _env, info, lpt_amount),
        ExecuteMsg::Swap { denom, amount } => execute::swap(deps, _env, info, denom, amount),
    }
}

pub fn get_cw20_token_allowance (
    deps: &DepsMut,
    owner: String, 
    spender: String, 
    token_contract: String,
) -> StdResult<Uint128> {

    let response: AllowanceResponse = deps.querier.query(&cosmwasm_std::QueryRequest::Wasm(cosmwasm_std::WasmQuery::Smart {
        contract_addr: token_contract.to_string(),
        msg: to_json_binary(&cw20::Cw20QueryMsg::Allowance { owner: owner.to_string(), spender: spender.to_string() })?,
    }))?;
    
    Ok(response.allowance)
}

pub fn calculate_swap_amount(
    pool: &LiquidityPool,
    denom: &String, 
    amount: Uint128,
)-> StdResult<Uint128> {
    if denom.as_str() == DENOM_ORAI {
        let orai_reserve = pool.orai_reserve.u128();
        let usdt_reserve = pool.usdt_reserve.u128();
        let amount_with_fee = amount.u128() * 997 / 1000; // 0.3% fee
        let numerator = amount_with_fee * usdt_reserve;
        let denominator = orai_reserve + amount_with_fee;
        Ok(Uint128::from(numerator / denominator))
    } else if denom.as_str() == DENOM_USDT {
        let orai_reserve = pool.orai_reserve.u128();
        let usdt_reserve = pool.usdt_reserve.u128();
        let amount_with_fee = amount.u128() * 997 / 1000; // 0.3% fee
        let numerator = amount_with_fee * orai_reserve;
        let denominator = usdt_reserve + amount_with_fee;
        Ok(Uint128::from(numerator / denominator))
    } else {
        Err(StdError::generic_err("Unsupported token pair"))
    }
}


pub fn transfer_orai(
    recipient: String,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    Ok(CosmosMsg::Bank(BankMsg::Send {
        to_address: recipient.to_string(),
        amount: vec![Coin {
            denom: "orai".to_string(),
            amount,
        }],
    }))
}

pub fn transfer_usdt(
    deps: &DepsMut,
    recipient: String, 
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    let contract_info = INFO.load(deps.storage)?;
    let transfer_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: contract_info.usdt_contract.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
            recipient: recipient.to_string(),
            amount,
        })?,
        funds: vec![],
    });

    Ok(transfer_msg)
}
pub fn transfer_token(
    deps: DepsMut,
    denom: String, 
    recipient: String, 
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    let contract_info = INFO.load(deps.storage)?;
    let token_contract = if denom.as_str() == DENOM_USDT {
        contract_info.usdt_contract
    } else if denom.as_str() == DENOM_ORAI {
        return transfer_orai(recipient, amount);
    } else {
        return Err(StdError::generic_err("Unsupported token"));
    };
    let transfer_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_contract.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
            recipient: recipient.to_string(),
            amount,
        })?,
        funds: vec![],
    });

    Ok(transfer_msg)
}



pub mod execute {

    use crate::state::{LIQUIDITY_PROVIDERS, POOL};

    use super::*;

    pub fn add_liquidity(
        deps: DepsMut,
        env: Env, 
        info: MessageInfo,
        orai_amount: Uint128, 
        usdt_amount: Uint128, 
    ) -> Result<Response, ContractError> {

        let contract_info = INFO.load(deps.storage)?;

        //1.sure orai_amount & usdt_amount != 0 
        if orai_amount.is_zero() || usdt_amount.is_zero() {
            return Err(ContractError::InvalidTokenAmount {});
        };

        //2.check received orai in message, orai_amount must equal received orai
        let received_orai = info
            .funds
            .iter()
            .find(|coin| coin.denom == "orai") // Assuming "orai" is the denom for the native ORAI token
            .map(|coin| coin.amount.clone()) // Retrieve the amount of ORAI received
            .unwrap_or_default(); // If ORAI is not found, return default value (zero)

        if received_orai != orai_amount {
            return Err(ContractError::InvalidTokenAmount {});
        };

        //3.check amount_usdt allowance for contract, sure it equal to usdt_amount
        // let allowance = get_usdt_allowance(&deps, info.sender.clone().to_string(), env.contract.address.clone().to_string())?;
        let allowance = get_cw20_token_allowance(
            &deps, 
            info.sender.clone().to_string(), 
            env.contract.address.to_string(), 
            contract_info.usdt_contract.to_string())?;

        if allowance != usdt_amount {
            return Err(ContractError::InvalidTokenAmount {});
        };

        let mut pool = POOL.load(deps.storage)?;
        //4.check if usdt_reserve = 0 || orai_reserve = 0
        if pool.orai_reserve.is_zero() || pool.usdt_reserve.is_zero() {
            //update liquidity pool & mint lpt for sender
            pool.orai_reserve += orai_amount;
            pool.usdt_reserve += usdt_amount;

                //calculate lpt_mint
            let square_orai_amount = (orai_amount.u128() as f64).sqrt();
            let square_usdt_amount = (usdt_amount.u128() as f64).sqrt();

            let lpt_mint_in_f64 = square_orai_amount * square_usdt_amount;
            let lpt_mint = Uint128::new(lpt_mint_in_f64.round() as u128);

            pool.total_shares += lpt_mint;

            //mint_lpt for user
            let mint_msg = CosmosMsg::Wasm(WasmMsg::Execute { 
                contract_addr: contract_info.lpt_contract.to_string(), 
                msg: to_json_binary(&Cw20ExecuteMsg::Mint { 
                    recipient: info.sender.to_string(), 
                    amount: lpt_mint, 
                })?, 
                funds: vec![],
            });

            //update liquidity_providers for user 

            let sender = info.sender.clone();

            let current_lpt_balance = LIQUIDITY_PROVIDERS.may_load(deps.storage, &sender)?.unwrap_or_default();

            let new_lpt_balance = current_lpt_balance + lpt_mint;

            LIQUIDITY_PROVIDERS.save(deps.storage, &sender, &new_lpt_balance)?;
            //save deps storage pool
            POOL.save(deps.storage, &pool)?;

            return Ok(Response::new()
                .add_message(mint_msg)            
                .add_attribute("action", "add_liquidity")
                .add_attribute("orai_amouunt", orai_amount.to_string())
                .add_attribute("usdt_amount", usdt_amount.to_string())
                .add_attribute("lpt_mint", lpt_mint.to_string())
            );
        }

        
        //6.kiem tra orai_amount va usdt_amount, cu lay theo ty le nhung neu thua token nao thi phai gui lai cho nguoi dung 

        // let token_contract = if denom.as_str() == DENOM_USDT {
        //     contract_info.usdt_contract
        // } else if denom.as_str() == DENOM_ORAI {
        //     return Ok(transfer_orai(recipient, amount));
        // } else {
        //     return Err(StdError::generic_err("Unsupported token"));
        // };
        let unused_orai = if orai_amount.u128() * pool.usdt_reserve.u128() > usdt_amount.u128() * pool.orai_reserve.u128() {
            Uint128::from(
                (orai_amount.u128() * pool.usdt_reserve.u128() - usdt_amount.u128() * pool.orai_reserve.u128()) 
                / pool.usdt_reserve.u128()
            )
        } else {
            Uint128::zero()
        };

        let unused_usdt = if usdt_amount.u128() * pool.orai_reserve.u128() > orai_amount.u128() * pool.usdt_reserve.u128() {
            Uint128::from((usdt_amount.u128() * pool.orai_reserve.u128()) - (orai_amount.u128() * pool.usdt_reserve.u128()) / pool.orai_reserve.u128())
        } else {
            Uint128::zero()
        };

        let orai_amount = orai_amount - unused_orai;

        let usdt_amount = usdt_amount - unused_usdt;

        let msg_transfer_unused_token = if orai_amount.u128() * pool.usdt_reserve.u128() > usdt_amount.u128() * pool.orai_reserve.u128() {
            
            transfer_orai(info.sender.clone().to_string(), unused_orai)
        } else {
           
            transfer_usdt(&deps, info.sender.clone().to_string(), unused_usdt)
        }?;


        //7. If 6. true

        //calculate lpt_mint
        let lpt_mint = Uint128::from(orai_amount * pool.total_shares / pool.orai_reserve);

        //update pool state
        pool.orai_reserve += orai_amount;
        pool.usdt_reserve += usdt_amount;
        pool.total_shares += lpt_mint;

        let mint_msg = CosmosMsg::Wasm(WasmMsg::Execute { 
            contract_addr: contract_info.lpt_contract.to_string(), 
            msg: to_json_binary(&Cw20ExecuteMsg::Mint { 
                recipient: info.sender.to_string(), 
                amount: lpt_mint, 
            })?, 
            funds: vec![],
        });
        //update liquidity_providers for user

        let sender = info.sender.clone();

        let current_lpt_balance = LIQUIDITY_PROVIDERS.may_load(deps.storage, &sender)?.unwrap_or_default();

        let new_lpt_balance = current_lpt_balance + lpt_mint;

        LIQUIDITY_PROVIDERS.save(deps.storage, &sender, &new_lpt_balance)?;
        //save pool state
        POOL.save(deps.storage, &pool)?;
        Ok(Response::new()
            // .add_message(msg_return_token)
            .add_message(msg_transfer_unused_token)
            .add_message(mint_msg)
            .add_attribute("action", "add_liquidity")
            .add_attribute("orai_amouunt", orai_amount.to_string())
            .add_attribute("usdt_amount", usdt_amount.to_string())
            .add_attribute("lpt_mint", lpt_mint.to_string()))
    }

    pub fn remove_liquidity(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        lpt_amount: Uint128,
    ) -> Result<Response, ContractError> {
        let contract_info = INFO.load(deps.storage)?;
        //sure lpt_amount != 0
        if lpt_amount.is_zero() {
            return Err(ContractError::InvalidTokenAmount {  });
        };

        //get approved lpt from user 
        let approved_lpt = get_cw20_token_allowance(
            &deps, 
            info.sender.clone().to_string(), 
            env.contract.address.to_string(), 
            contract_info.lpt_contract.to_string(),
        )?;
        //sure approved_lpt >= lpt_amount 
        if approved_lpt < lpt_amount {
            return Err(ContractError::InvalidTokenAmount {  });
        }
        //sure lpt_balance >= lpt_amount
            //get lpt_balance 
        let sender = info.sender.clone();
        let current_lpt_balance = LIQUIDITY_PROVIDERS.may_load(deps.storage, &sender)?.unwrap_or_default();
        if current_lpt_balance < lpt_amount {
            return  Err(ContractError::InvalidTokenAmount {  });
        }
        //calculate orai_amount, usdt_amount
        let mut pool = POOL.load(deps.storage)?;

        let orai_amount = Uint128::from(lpt_amount * pool.orai_reserve / pool.total_shares);
        let usdt_amount = Uint128::from(lpt_amount * pool.usdt_reserve / pool.total_shares);

        //update pool state
        pool.orai_reserve -= orai_amount;
        pool.usdt_reserve -= usdt_amount;
        pool.total_shares -= lpt_amount;
        
        POOL.save(deps.storage, &pool)?;
        //update lpt balance in LIQUIDITY_PROVIDER
        let new_lpt_balance = current_lpt_balance - lpt_amount;

        LIQUIDITY_PROVIDERS.save(deps.storage, &sender, &new_lpt_balance)?;

        //burn lpt from user (user must approve lpt_amount for contract)
        let burn_msg = CosmosMsg::Wasm(WasmMsg::Execute { 
            contract_addr: contract_info.lpt_contract.to_string(), 
            msg: to_json_binary(&Cw20ExecuteMsg::BurnFrom { 
                owner: info.sender.to_string(), 
                amount: lpt_amount })?, 
            funds: vec![] });
        //transfer orai, usdt for user

            // Transfer ORAI (native token) & USDT for user
 

        let usdt_transfer_msg = transfer_usdt(&deps, info.sender.clone().to_string(), usdt_amount)?;
        let orai_transfer_msg = transfer_orai(info.sender.clone().to_string(), orai_amount)?;

        Ok(Response::new()
            .add_message(burn_msg)
            .add_message(orai_transfer_msg)
            .add_message(usdt_transfer_msg)
            .add_attribute("action", "remove_liquidity")
            .add_attribute("lpt_amount", lpt_amount.to_string())
            .add_attribute("receive_usdt", usdt_amount.to_string())
            .add_attribute("receive_orai", orai_amount.to_string()))
    }

    pub fn swap(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        denom: String,
        amount: Uint128,   
    ) -> Result<Response, ContractError> {
        let contract_info= INFO.load(deps.storage)?;
        //check if amount == 0
        if amount.is_zero() {
            return Err(ContractError::InvalidTokenAmount {  });
        }
        
        //check approved usdt or amount of orai if message
        

        if denom.as_str() == DENOM_ORAI {
            let received_orai = info
                .funds
                .iter()
                .find(|coin| coin.denom == "orai") // Assuming "orai" is the denom for the native ORAI token
                .map(|coin| coin.amount.clone()) // Retrieve the amount of ORAI received
                .unwrap_or_default(); // If ORAI is not found, return default value (zero)
            if received_orai != amount {
                return Err(ContractError::InvalidTokenAmount {});
            }
        } else if denom.as_str() == DENOM_USDT {
            let approved_usdt = get_cw20_token_allowance(
                &deps, 
                info.sender.clone().to_string(), 
                env.contract.address.clone().to_string(), 
                contract_info.usdt_contract.to_string())?;
    
            if approved_usdt != amount {
                return Err(ContractError::InvalidTokenAmount {});
            }
        } else {
            return Err(ContractError::InvalidTokenAmount {});
        }
        //count amount swap 
        let mut pool = POOL.load(deps.storage)?;
        
        let swap_amount = calculate_swap_amount(&pool, &denom, amount)?;

        //sure enough token with denom type for swap
        if (denom.as_str() == DENOM_ORAI && swap_amount > pool.usdt_reserve)
            || (denom.as_str() == DENOM_USDT && swap_amount > pool.orai_reserve) 
        {
            return Err(ContractError::InsufficientLiquidity {});
        }

        //update pool state
        if denom.as_str() == DENOM_ORAI {
            pool.orai_reserve += amount;
            pool.usdt_reserve -= swap_amount;
        } else if denom.as_str() == DENOM_USDT {
            pool.usdt_reserve += amount;
            pool.orai_reserve -= swap_amount;
        } else {
            return Err(ContractError::InvalidTokenPair {});
        }

        POOL.save(deps.storage, &pool)?;

        let transfer_msg = transfer_token(deps, denom, info.sender.clone().to_string(), swap_amount)?;
        Ok(Response::new()
            .add_message(transfer_msg)
            .add_attribute("key", "123"))
    }
}

// #[cfg_attr(not(feature = "library"), entry_point)]
// pub fn query(
//     deps: DepsMut,
//     _env: Env,
//     // info: MessageInfo,
//     msg: QueryMsg,
// ) -> StdResult<Binary> {
//     // unimplemented!()
//     match msg {
//         QueryMsg::QueryContractInfo { } => to_json_binary(&query_contract_info(deps)?), //to_json_binary(&query_marketing_info(deps)?),
//         QueryMsg::QueryPoolInfo {  } =>  to_json_binary(&query_liquidity_pool_info(deps)?),
//         QueryMsg::QueryLptBalance { user } => to_json_binary(&query_lpt_balance(deps, user)?),
//     }
// }


// pub fn query_contract_info(deps: DepsMut) -> StdResult<ContractInfoResponse>{
//     let ct_info = INFO.load(deps.storage)?;
//     Ok(ContractInfoResponse { owner: ct_info.owner.to_string(), lpt_contract: ct_info.lpt_contract, usdt_contract: ct_info.usdt_contract })
// }

// pub fn query_liquidity_pool_info(deps: DepsMut) -> StdResult<PoolInfoResponse> {
//     let pool = POOL.load(deps.storage)?;

//     Ok(PoolInfoResponse { orai_reserve: pool.orai_reserve, usdt_reserve: pool.usdt_reserve, total_shares: pool.total_shares })
// }

// pub fn query_lpt_balance(deps: DepsMut, user: String) -> StdResult<LptBalanceResponse> {
//     // Convert the user string to Addr
//     let user_addr = deps.api.addr_validate(&user)?;

//         // Query the balance from the LIQUIDITY_PROVIDERS map
//     let balance = LIQUIDITY_PROVIDERS
//         .load(deps.storage, &user_addr)
//         .unwrap_or(Uint128::zero());

//     // Return the response
//     Ok(LptBalanceResponse { balance })
// }

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(
    deps: Deps, // Use Deps instead of DepsMut
    _env: Env,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::QueryContractInfo {} => to_json_binary(&query_contract_info(deps)?),
        QueryMsg::QueryPoolInfo {} => to_json_binary(&query_liquidity_pool_info(deps)?),
        QueryMsg::QueryLptBalance { user } => to_json_binary(&query_lpt_balance(deps, user)?),
    }
}

pub fn query_contract_info(deps: Deps) -> StdResult<ContractInfoResponse> {
    let ct_info = INFO.load(deps.storage)?;
    Ok(ContractInfoResponse {
        owner: ct_info.owner.to_string(),
        lpt_contract: ct_info.lpt_contract,
        usdt_contract: ct_info.usdt_contract,
    })
}

pub fn query_liquidity_pool_info(deps: Deps) -> StdResult<PoolInfoResponse> {
    let pool = POOL.load(deps.storage)?;
    Ok(PoolInfoResponse {
        orai_reserve: pool.orai_reserve,
        usdt_reserve: pool.usdt_reserve,
        total_shares: pool.total_shares,
    })
}

pub fn query_lpt_balance(deps: Deps, user: String) -> StdResult<LptBalanceResponse> {
    // Convert the user string to Addr
    let user_addr = deps.api.addr_validate(&user)?;

    // Query the balance from the LIQUIDITY_PROVIDERS map
    let balance = LIQUIDITY_PROVIDERS
        .load(deps.storage, &user_addr)
        .unwrap_or(Uint128::zero());

    // Return the response
    Ok(LptBalanceResponse { balance })
}

#[cfg(test)]
mod tests {}
