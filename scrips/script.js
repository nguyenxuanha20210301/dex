import * as cosmwasm from "@cosmjs/cosmwasm-stargate";
import dotenv from "dotenv";
// import { CHAIN_CONFIG, CONFIG, CONTRACT_ADDR } from "../config";
import { GasPrice } from "@cosmjs/stargate";
import { SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import { Decimal } from "@cosmjs/math";
import { Secp256k1HdWallet } from "@cosmjs/amino";
import fs from "fs";
// This is your rpc endpoint
// const rpcEndpoint = "https://rpc.orai.io"

dotenv.config();

const mnemonic = process.env.MNEMONIC

console.log(mnemonic);


export const getSignCosmwasmClient = async () => {
    const denom = "orai";
  
    const wallet = await Secp256k1HdWallet.fromMnemonic(process.env.MNEMONIC, {
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
    console.log(contract_address);
    
    const fee = "auto"
    //=====================================DEPLOY========================================

    // //wasm -> wasmCode
    // const path = "./artifacts/dex.wasm"
    // const wasmCode = new Uint8Array(fs.readFileSync(path))

    // console.log("UPload !!!!!!!!!!!!!!!!!!!!!");
    
    // // //upload code on chain
    // const upload = await client.upload(sender, wasmCode, fee)
    // console.log(upload)
    
    
    // instantiate msg

    // const instantiate_msg = {
    //     usdt_contract: process.env.USDT_CONTRACT,
    //     lpt_contract: process.env.LPT_CONTRACT,
    // };
    
    // const res = await client.instantiate(sender, upload.codeId, instantiate_msg, "dex", fee)
    // console.log(res)

    //===================================================================================
    // ===================================================================================


    // =====================================EXECUTE=======================================
    

    // const execute_mint_msg = {
    //     mint: {
    //         recipient: address,  // Địa chỉ nhận token
    //         amount: "500",
    //     }
    // };
    // const mint_response = await client.execute(address, contract_address, execute_mint_msg, fee);
    // console.log("Mint Response:", mint_response);
    //===================================================================================

    //======================================QUERY========================================

    // const query_example = await client.queryContractSmart(
    //     contract_address, {example: {}})
    // console.log(query_example)

    // const query_balance_msg = {
    //     balance: {
    //         address: address
    //     }
    // };
    // const balance_response = await client.queryContractSmart(contract_address, query_balance_msg);
    // console.log("CW20 OCH Token Balance:", balance_response);

    //===================================================================================
}


main();