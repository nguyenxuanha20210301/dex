import * as cosmwasm from "@cosmjs/cosmwasm-stargate";
import dotenv from "dotenv";
// import { CHAIN_CONFIG, CONFIG, CONTRACT_ADDR } from "../config";
import { GasPrice } from "@cosmjs/stargate";
import { SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import { Decimal } from "@cosmjs/math";
import { Secp256k1HdWallet } from "@cosmjs/amino";
import fs from "fs";
// import { send } from "process";
// This is your rpc endpoint
// const rpcEndpoint = "https://rpc.orai.io"

dotenv.config();

const mnemonic = process.env.MNEMONIC_WALLET1

// console.log(mnemonic);

export const getSignCosmwasmClient = async () => {
    const denom = "orai";
  
    const wallet = await Secp256k1HdWallet.fromMnemonic(mnemonic, {
      prefix: "orai",
    });
  
    const client = await cosmwasm.SigningCosmWasmClient.connectWithSigner(
      process.env.RPC_URL_MAINNET,
      wallet,
      {
        gasPrice: new GasPrice(Decimal.fromUserInput("0.001", 6), denom),
      }
    );
  
    return { client, sender: (await wallet.getAccounts())[0].address };
  };
async function main() {
    const {client, sender} = await getSignCosmwasmClient();

    
    console.log(await client.getBalance(sender, "orai"))

    // địa chỉ ví contract sau khi đã deploy
    const contract_address = process.env.CONTRACT_ADDRESS_MAINNET;
    // console.log(contract_address);
    
    const fee = "auto"
    //=====================================DEPLOY========================================

    // // //wasm -> wasmCode
    // const path = "./artifacts/dex.wasm"
    // const wasmCode = new Uint8Array(fs.readFileSync(path))

    
    // // //upload code on chain
    // const upload = await client.upload(sender, wasmCode, fee)
    // console.log(upload)
    
    // // //instantiate msg

    // const instantiate_msg = {
    //     usdt_contract: process.env.USDT_CONTRACT,
    //     lpt_contract: process.env.LPT_CONTRACT,
    // };
    
    // const res = await client.instantiate(sender, upload.codeId, instantiate_msg, "dex", fee)
    // console.log(res)

    //===================================================================================
    // ===================================================================================


    // =====================================EXECUTE=======================================
    
    console.log("execute msg");
    
    // const execute_msg = {
    //   add_liquidity: {
    //     orai_amount: "100",
    //     usdt_amount: "400"
    //   }
    // };
    
    // const funds = [{
    //   denom: "orai",       // loại token là 'orai'
    //   amount: "100"       // số lượng orai bạn muốn chuyển (1000 orai)
    // }];
    
    // const execute_response = await client.execute(sender, contract_address, execute_msg, fee, "", funds);
    // console.log(execute_response);

    // const exe1 = {
    //   remove_liquidity: {
    //     lpt_amount: "30"
    //   }
    // }
    // const res1 = await client.execute(sender, contract_address, exe1, fee);
    // console.log(res1);

    // const exe2 = {
    //   swap: {
    //     denom: "orai", 
    //     amount: "150"
    //   }
    // }
    // const funds = [{
    //   denom: "orai",       // loại token là 'orai'
    //   amount: "150"       // số lượng orai bạn muốn chuyển (1000 orai)
    // }];
    // const res2 = await client.execute(sender, contract_address, exe2, fee, "", funds);
    // console.log(res2);
    //===================================================================================

    //======================================QUERY========================================
    console.log("query msg");


    const query_msg1 = {
      query_pool_info: {}
    }
    const query_contract_info = await client.queryContractSmart(contract_address, query_msg1);
    console.log(query_contract_info);

    const query_msg2= {
      query_contract_info: {}
    }
    const query_contract_info1 = await client.queryContractSmart(contract_address, query_msg2);
    console.log(query_contract_info1);

    const query_msg3 = {
      query_u_s_d_t_allowance: {
        user: sender
      }
    }
    const query_res = await client.queryContractSmart(contract_address, query_msg3);
    console.log(query_res);
    //===================================================================================
}


main();