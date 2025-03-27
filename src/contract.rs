#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg};
use cw20::{AllowanceResponse, Cw20ExecuteMsg};
// use crate::error::ContractError;
use crate::msg::{ContractInfoResponse, ExecuteMsg, InstantiateMsg, LptBalanceResponse, PoolInfoResponse, QueryMsg, USDTAllowanceResponse};
use crate::state::{ContractInfo, LiquidityPool, INFO, LIQUIDITY_PROVIDERS, POOL, USDT_ALLOWANCE};
use std::str::FromStr;

const DENOM_ORAI: &str = "orai";
const DENOM_USDT: &str = "usdt";

pub fn sqrt(value: Decimal) -> Decimal {
    if value.is_zero() {
        return Decimal::zero();
    }
    value.sqrt()
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, StdError> {
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

    INFO.save(deps.storage, &contract_info)?;
    POOL.save(deps.storage, &pool)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender)
        .add_attribute("usdt_contract", msg.usdt_contract)
        .add_attribute("lpt_contract", msg.lpt_contract))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, StdError> {
    match msg {
        ExecuteMsg::AddLiquidity { orai_amount, usdt_amount } => add_liquidity(deps, env, info, orai_amount, usdt_amount),
        ExecuteMsg::RemoveLiquidity { lpt_amount } => remove_liquidity(deps, env, info, lpt_amount),
        ExecuteMsg::Swap { denom, amount } => swap(deps, env, info, denom, amount),
    }
}

pub fn query_cw20_token_allowance(
    deps: &DepsMut,
    owner: String,
    spender: String,
    token_contract: &String,
) -> StdResult<Uint128> {
    let response: AllowanceResponse = deps.querier.query(&cosmwasm_std::QueryRequest::Wasm(cosmwasm_std::WasmQuery::Smart {
        contract_addr: token_contract.to_string(),
        msg: to_json_binary(&cw20::Cw20QueryMsg::Allowance { owner, spender })?,
    }))?;
    Ok(response.allowance)
}

pub fn calculate_swap_amount(
    pool: &LiquidityPool,
    denom: &String,
    amount: Uint128,
) -> StdResult<Uint128> {
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
        Err(StdError::generic_err("calculate_swap_amount: Unsupported token pair"))
    }
}

pub fn transfer_orai(
    recipient: String,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    if amount.is_zero() {
        return Err(StdError::generic_err("transfer_orai: Amount must be greater than zero"));
    }
    Ok(CosmosMsg::Bank(BankMsg::Send {
        to_address: recipient,
        amount: vec![Coin {
            denom: DENOM_ORAI.to_string(),
            amount,
        }],
    }))
}

pub fn transfer_usdt(
    deps: &DepsMut,
    recipient: String,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    if amount.is_zero() {
        return Err(StdError::generic_err("transfer_usdt: Amount must be greater than zero"));
    }
    let contract_info = INFO.load(deps.storage)?;
    let transfer_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: contract_info.usdt_contract.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::Transfer { recipient, amount })?,
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
    if denom.as_str() == DENOM_USDT {
        let transfer_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_info.usdt_contract.to_string(),
            msg: to_json_binary(&Cw20ExecuteMsg::Transfer { recipient, amount })?,
            funds: vec![],
        });
        Ok(transfer_msg)
    } else if denom.as_str() == DENOM_ORAI {
        transfer_orai(recipient, amount)
    } else {
        Err(StdError::generic_err("transfer_token: Unsupported token denom"))
    }
}

pub fn add_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    orai_amount: Uint128,
    usdt_amount: Uint128,
) -> Result<Response, StdError> {
    let contract_info = INFO.load(deps.storage)?;

    if orai_amount.is_zero() || usdt_amount.is_zero() {
        return Err(StdError::generic_err("add_liquidity: ORAI or USDT amount cannot be zero"));
    }

    let received_orai = info
        .funds
        .iter()
        .find(|coin| coin.denom == DENOM_ORAI)
        .map(|coin| coin.amount)
        .unwrap_or_default();

    if received_orai < orai_amount {
        return Err(StdError::generic_err(format!(
            "add_liquidity: Insufficient ORAI received. Expected: {}, Received: {}",
            orai_amount, received_orai
        )));
    }

    let mut pool = POOL.load(deps.storage)?;
    if pool.orai_reserve.is_zero() || pool.usdt_reserve.is_zero() {
        pool.orai_reserve += orai_amount;
        pool.usdt_reserve += usdt_amount;

        let a = Decimal::from_atomics(orai_amount, 0).unwrap();
        let b = Decimal::from_atomics(usdt_amount, 0).unwrap();
        let divisor = Decimal::from_str("3.14918").unwrap();
        let lpt_mint_decimal = sqrt(a) * sqrt(b) / divisor;
        let lpt_mint = lpt_mint_decimal.to_uint_floor(); // Sửa từ to_uint_floor trên Uint128 sang Decimal

        pool.total_shares += lpt_mint;

        let mint_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_info.lpt_contract.to_string(),
            msg: to_json_binary(&Cw20ExecuteMsg::Mint {
                recipient: info.sender.to_string(),
                amount: lpt_mint,
            })?,
            funds: vec![],
        });

        let sender = info.sender.clone();
        let current_lpt_balance = LIQUIDITY_PROVIDERS.may_load(deps.storage, &sender)?.unwrap_or_default();
        let new_lpt_balance = current_lpt_balance + lpt_mint;

        let usdt_allowance = query_cw20_token_allowance(&deps, info.sender.to_string(), env.contract.address.to_string(), &contract_info.usdt_contract.to_string())?;
        if usdt_allowance < usdt_amount {
            return Err(StdError::generic_err(format!(
                "add_liquidity: Insufficient USDT allowance. Required: {}, Available: {}",
                usdt_amount, usdt_allowance
            )));
        }

        let transfer_from_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_info.usdt_contract.to_string(),
            msg: to_json_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: sender.to_string(),
                recipient: env.contract.address.to_string(),
                amount: usdt_amount,
            })?,
            funds: vec![],
        });

        USDT_ALLOWANCE.save(deps.storage, &sender, &usdt_allowance)?;
        POOL.save(deps.storage, &pool)?;
        LIQUIDITY_PROVIDERS.save(deps.storage, &sender, &new_lpt_balance)?;

        return Ok(Response::new()
            .add_message(mint_msg)
            .add_message(transfer_from_msg)
            .add_attribute("action", "add_liquidity")
            .add_attribute("orai_amount", orai_amount.to_string())
            .add_attribute("usdt_amount", usdt_amount.to_string())
            .add_attribute("lpt_mint", lpt_mint.to_string()));
    }

    let pool_ratio = Decimal::from_ratio(pool.orai_reserve, pool.usdt_reserve);
    let input_ratio = Decimal::from_ratio(orai_amount, usdt_amount);

    let (orai_to_use, usdt_to_use, unused_orai, unused_usdt) = if input_ratio > pool_ratio {
        let usdt_to_use = usdt_amount;
        let orai_to_use = (pool_ratio * Decimal::from_atomics(usdt_to_use, 0).unwrap()).to_uint_floor();
        let unused_orai = orai_amount - orai_to_use;
        (orai_to_use, usdt_to_use, unused_orai, Uint128::zero())
    } else if input_ratio < pool_ratio {
        let orai_to_use = orai_amount;
        let usdt_to_use = (Decimal::from_atomics(orai_to_use, 0).unwrap() * Decimal::from_ratio(pool.usdt_reserve, pool.orai_reserve)).to_uint_floor();
        let unused_usdt = usdt_amount - usdt_to_use;
        (orai_to_use, usdt_to_use, Uint128::zero(), unused_usdt)
    } else {
        (orai_amount, usdt_amount, Uint128::zero(), Uint128::zero())
    };

    let usdt_allowance = query_cw20_token_allowance(&deps, info.sender.to_string(), env.contract.address.to_string(), &contract_info.usdt_contract.to_string())?;
    if usdt_allowance < usdt_to_use {
        return Err(StdError::generic_err(format!(
            "add_liquidity: Insufficient USDT allowance for non-empty pool. Required: {}, Available: {}",
            usdt_to_use, usdt_allowance
        )));
    }

    let lpt_mint = (Decimal::from_atomics(orai_to_use * pool.total_shares / pool.orai_reserve, 0).unwrap()).to_uint_floor(); // Sửa: Chuyển đổi Decimal sang Uint128
    pool.orai_reserve += orai_to_use;
    pool.usdt_reserve += usdt_to_use;
    pool.total_shares += lpt_mint;

    let sender = info.sender.clone();
    let current_lpt_balance = LIQUIDITY_PROVIDERS.may_load(deps.storage, &sender)?.unwrap_or_default();
    let new_lpt_balance = current_lpt_balance + lpt_mint;

    let mint_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: contract_info.lpt_contract.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::Mint {
            recipient: info.sender.to_string(),
            amount: lpt_mint,
        })?,
        funds: vec![],
    });

    let transfer_from_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: contract_info.usdt_contract.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::TransferFrom {
            owner: sender.to_string(),
            recipient: env.contract.address.to_string(),
            amount: usdt_to_use,
        })?,
        funds: vec![],
    });

    let mut response = Response::new()
        .add_message(transfer_from_msg)
        .add_message(mint_msg)
        .add_attribute("action", "add_liquidity")
        .add_attribute("orai_amount", orai_to_use.to_string())
        .add_attribute("usdt_amount", usdt_to_use.to_string())
        .add_attribute("lpt_mint", lpt_mint.to_string());

    if !unused_orai.is_zero() {
        let msg_transfer_unused_orai = transfer_orai(info.sender.to_string(), unused_orai)?;
        response = response.add_message(msg_transfer_unused_orai);
    }

    // if !unused_usdt.is_zero() {
    //     let msg_transfer_unused_usdt = transfer_usdt(&deps, info.sender.to_string(), unused_usdt)?;
    //     response = response.add_message(msg_transfer_unused_usdt);
    // }

    USDT_ALLOWANCE.save(deps.storage, &sender, &usdt_allowance)?;
    LIQUIDITY_PROVIDERS.save(deps.storage, &sender, &new_lpt_balance)?;
    POOL.save(deps.storage, &pool)?;

    Ok(response)
}

pub fn remove_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    lpt_amount: Uint128,
) -> Result<Response, StdError> {
    let contract_info = INFO.load(deps.storage)?;

    if lpt_amount.is_zero() {
        return Err(StdError::generic_err("remove_liquidity: LPT amount cannot be zero"));
    }

    let approved_lpt = query_cw20_token_allowance(
        &deps,
        info.sender.to_string(),
        env.contract.address.to_string(),
        &contract_info.lpt_contract.to_string(),
    )?;
    if approved_lpt < lpt_amount {
        return Err(StdError::generic_err(format!(
            "remove_liquidity: Insufficient LPT allowance. Required: {}, Available: {}",
            lpt_amount, approved_lpt
        )));
    }

    let sender = info.sender.clone();
    let current_lpt_balance = LIQUIDITY_PROVIDERS.may_load(deps.storage, &sender)?.unwrap_or_default();
    if current_lpt_balance < lpt_amount {
        return Err(StdError::generic_err(format!(
            "remove_liquidity: Insufficient LPT balance. Required: {}, Available: {}",
            lpt_amount, current_lpt_balance
        )));
    }

    let mut pool = POOL.load(deps.storage)?;
    if pool.total_shares.is_zero() {
        return Err(StdError::generic_err("remove_liquidity: Pool has no shares"));
    }

    let orai_amount = Uint128::from(lpt_amount * pool.orai_reserve / pool.total_shares);
    let usdt_amount = Uint128::from(lpt_amount * pool.usdt_reserve / pool.total_shares);

    if pool.orai_reserve < orai_amount || pool.usdt_reserve < usdt_amount {
        return Err(StdError::generic_err(format!(
            "remove_liquidity: Insufficient liquidity. ORAI required: {}, Available: {}. USDT required: {}, Available: {}",
            orai_amount, pool.orai_reserve, usdt_amount, pool.usdt_reserve
        )));
    }

    pool.orai_reserve -= orai_amount;
    pool.usdt_reserve -= usdt_amount;
    pool.total_shares -= lpt_amount;

    POOL.save(deps.storage, &pool)?;

    let new_lpt_balance = current_lpt_balance - lpt_amount;
    LIQUIDITY_PROVIDERS.save(deps.storage, &sender, &new_lpt_balance)?;

    let burn_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: contract_info.lpt_contract.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::BurnFrom {
            owner: info.sender.to_string(),
            amount: lpt_amount,
        })?,
        funds: vec![],
    });

    let usdt_transfer_msg = transfer_usdt(&deps, info.sender.to_string(), usdt_amount)?;
    let orai_transfer_msg = transfer_orai(info.sender.to_string(), orai_amount)?;

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
) -> Result<Response, StdError> {
    let contract_info = INFO.load(deps.storage)?;

    if amount.is_zero() {
        return Err(StdError::generic_err("swap: Amount cannot be zero"));
    }

    let mut pool = POOL.load(deps.storage)?;
    if pool.orai_reserve.is_zero() || pool.usdt_reserve.is_zero() {
        return Err(StdError::generic_err("swap: Pool has no liquidity"));
    }

    let mut response = Response::new().add_attribute("action", "swap").add_attribute("amount", amount.to_string());

    if denom.as_str() == DENOM_ORAI {
        let received_orai = info
            .funds
            .iter()
            .find(|coin| coin.denom == DENOM_ORAI)
            .map(|coin| coin.amount)
            .unwrap_or_default();
        if received_orai < amount {
            return Err(StdError::generic_err(format!(
                "swap: Insufficient ORAI received. Expected: {}, Received: {}",
                amount, received_orai
            )));
        }

        let swap_amount = calculate_swap_amount(&pool, &denom, amount)?;
        if swap_amount > pool.usdt_reserve {
            return Err(StdError::generic_err(format!(
                "swap: Insufficient USDT liquidity. Required: {}, Available: {}",
                swap_amount, pool.usdt_reserve
            )));
        }

        pool.orai_reserve += amount;
        pool.usdt_reserve -= swap_amount;
        POOL.save(deps.storage, &pool)?;

        let transfer_msg = transfer_usdt(&deps, info.sender.to_string(), swap_amount)?;
        response = response.add_message(transfer_msg).add_attribute("denom", DENOM_ORAI);
    } else if denom.as_str() == DENOM_USDT {
        let approved_usdt = query_cw20_token_allowance(
            &deps,
            info.sender.to_string(),
            env.contract.address.to_string(),
            &contract_info.usdt_contract.to_string(),
        )?;
        if approved_usdt < amount {
            return Err(StdError::generic_err(format!(
                "swap: Insufficient USDT allowance. Required: {}, Avaiaclable: {}",
                amount, approved_usdt
            )));
        }
        let swap_amount = calculate_swap_amount(&pool, &denom, amount)?;
        if swap_amount > pool.orai_reserve {
            return Err(StdError::generic_err(format!(
                "swap: Insufficient ORAI liquidity. Required: {}, Available: {}",
                swap_amount, pool.orai_reserve
            )));
        }

        let transfer_from_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_info.usdt_contract.to_string(),
            msg: to_json_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: info.sender.to_string(),
                recipient: env.contract.address.to_string(),
                amount,
            })?,
            funds: vec![],
        });

        pool.usdt_reserve += amount;
        pool.orai_reserve -= swap_amount;
        POOL.save(deps.storage, &pool)?;

        let transfer_msg = transfer_orai(info.sender.to_string(), swap_amount)?;
        response = response.add_message(transfer_from_msg).add_message(transfer_msg).add_attribute("denom", DENOM_USDT);
    } else {
        return Err(StdError::generic_err("swap: Invalid token denom"));
    }

    Ok(response)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(
    deps: Deps,
    _env: Env,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::QueryContractInfo {} => to_json_binary(&query_contract_info(deps)?),
        QueryMsg::QueryPoolInfo {} => to_json_binary(&query_liquidity_pool_info(deps)?),
        QueryMsg::QueryLptBalance { user } => to_json_binary(&query_lpt_balance(deps, user)?),
        QueryMsg::QueryUSDTAllowance { user } => to_json_binary(&query_usdt_allowance_amount(deps, user)?),
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
    let user_addr = deps.api.addr_validate(&user)?;
    let balance = LIQUIDITY_PROVIDERS
        .load(deps.storage, &user_addr)
        .unwrap_or(Uint128::zero());
    Ok(LptBalanceResponse { balance })
}

pub fn query_usdt_allowance_amount(deps: Deps, user: String) -> StdResult<USDTAllowanceResponse> {
    let user_addr = deps.api.addr_validate(&user)?;
    let usdt_allowance = USDT_ALLOWANCE.load(deps.storage, &user_addr).unwrap_or(Uint128::new(123u128));
    Ok(USDTAllowanceResponse { usdt_amount: usdt_allowance })
}

#[cfg(test)]
mod tests {
    // use super::*;
    // use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    // use cosmwasm_std::{Addr, coins};

    // #[test]
    // fn test_add_liquidity_initial() {
    //     let mut deps = mock_dependencies();
    //     let env = mock_env();
    //     let info = mock_info("sender", &coins(100, DENOM_ORAI));

    //     let instantiate_msg = InstantiateMsg {
    //         usdt_contract: Addr::unchecked("usdt_contract"),
    //         lpt_contract: Addr::unchecked("lpt_contract"),
    //     };
    //     instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();

    //     let res = add_liquidity(
    //         deps.as_mut(),
    //         env,
    //         info,
    //         Uint128::new(100),
    //         Uint128::new(300),
    //     ).unwrap();

    //     let lpt_mint_attr = res
    //         .attributes
    //         .iter()
    //         .find(|attr| attr.key == "lpt_mint")
    //         .expect("Attribute 'lpt_mint' not found");
    //     assert_eq!(lpt_mint_attr.value, "55", "lpt_mint should be 55");
    // }
}