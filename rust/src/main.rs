#![allow(unused)]
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::Amount;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::Write;

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

// You can use calls not provided in RPC lib API using the generic `call` function.
// An example of using the `send` RPC call, which doesn't have exposed API.
// You can also use serde_json `Deserialize` derivation to capture the returned json result.
fn send(rpc: &Client, addr: &str) -> bitcoincore_rpc::Result<String> {
    let args = [
        json!([{addr : 100 }]), // recipient address
        json!(null),            // conf target
        json!(null),            // estimate mode
        json!(null),            // fee rate in sats/vb
        json!(null),            // Empty option object
    ];

    #[derive(Deserialize)]
    struct SendResult {
        complete: bool,
        txid: String,
    }
    let send_result = rpc.call::<SendResult>("send", &args)?;
    assert!(send_result.complete);
    Ok(send_result.txid)
}

/// Returns an RPC client scoped to a single wallet.
///
/// Bitcoin Core serves per-wallet RPCs (getbalance, sendtoaddress, ...) under the
/// `/wallet/<name>` URL path, so a wallet needs a client pointed at that path.
fn wallet_client(name: &str) -> bitcoincore_rpc::Result<Client> {
    Client::new(
        &format!("{RPC_URL}/wallet/{name}"),
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )
}

/// Makes sure a wallet with the given name exists and is loaded.
///
/// The logic is idempotent so the program survives re-runs against a persistent
/// node: creation is attempted first, and if the wallet already exists on disk
/// the call falls back to loading it, treating an "already loaded" error as fine.
fn ensure_wallet(rpc: &Client, name: &str) {
    if rpc
        .create_wallet(name, None, None, None, None)
        .is_ok()
    {
        return;
    }
    // Already exists on disk (or already loaded) — try to load, ignore if loaded.
    let _ = rpc.load_wallet(name);
}

fn main() -> bitcoincore_rpc::Result<()> {
    // `rpc` is the node-level client; it talks to bitcoind but is not bound to
    // any wallet, so it is used for chain-wide calls like mining and mempool.
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // `getblockchaininfo` confirms the node is reachable and reports chain state.
    let blockchain_info = rpc.get_blockchain_info()?;
    println!("Blockchain Info: {:?}", blockchain_info);

    // The Miner and Trader wallets are the two actors in this scenario; their
    // names are case-sensitive and must match exactly.
    ensure_wallet(&rpc, "Miner");
    ensure_wallet(&rpc, "Trader");

    // Wallet-scoped RPCs (balance, sending) go through these per-wallet clients.
    let miner = wallet_client("Miner")?;
    let trader = wallet_client("Trader")?;
    println!("Wallets ready: Miner and Trader loaded.");

    // `mining_address` is where block rewards are paid; its "Mining Reward" label
    // makes the coinbase outputs easy to identify in the Miner wallet.
    let mining_address = miner
        .get_new_address(Some("Mining Reward"), None)?
        .assume_checked();

    // A block reward is a coinbase output, and consensus rules keep it unspendable
    // until it has 100 confirmations (COINBASE_MATURITY). The balance therefore
    // stays at zero until the first reward matures, which takes about 101 blocks.
    let mut blocks_mined = 0u64;
    loop {
        rpc.generate_to_address(1, &mining_address)?;
        blocks_mined += 1;
        if miner.get_balance(None, None)? > Amount::ZERO {
            break;
        }
    }
    println!("Mined {blocks_mined} blocks before Miner had a positive balance.");

    let miner_balance = miner.get_balance(None, None)?;
    println!("Miner wallet balance: {} BTC", miner_balance.to_btc());

    // `trader_address` (labelled "Received") is the Trader's destination for the
    // payment that follows.
    let trader_address = trader
        .get_new_address(Some("Received"), None)?
        .assume_checked();

    // The Miner pays 20 BTC to the Trader; the node picks inputs and adds change.
    let txid = miner.send_to_address(
        &trader_address,
        Amount::from_btc(20.0)?,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;
    println!("Sent 20 BTC from Miner to Trader, txid: {txid}");

    // While unconfirmed, the transaction sits in the mempool; its entry exposes
    // fee and size details before any block includes it.
    let mempool_entry = rpc.get_mempool_entry(&txid)?;
    println!("Mempool entry for {txid}: {mempool_entry:#?}");

    // Mining one block confirms the transaction; the returned hash identifies the
    // block it landed in.
    let block_hash = rpc.generate_to_address(1, &mining_address)?[0];
    println!("Confirmed in block {block_hash}");

    // `tx` is the decoded confirmed transaction, exposing its inputs and outputs.
    let tx = rpc.get_raw_transaction_info(&txid, None)?;

    // An input only points at a previous output by (txid, index); resolving that
    // previous output reveals the address funded earlier and the amount spent.
    let input = &tx.vin[0];
    let prev = rpc.get_raw_transaction_info(&input.txid.unwrap(), None)?;
    let prev_out = &prev.vout[input.vout.unwrap() as usize];
    let input_address = prev_out
        .script_pub_key
        .address
        .clone()
        .unwrap()
        .assume_checked()
        .to_string();
    let input_amount = prev_out.value.to_btc();

    // Of the two outputs, the one matching the Trader's address is the payment and
    // the other is the Miner's change.
    let trader_str = trader_address.to_string();
    let mut trader_out = (String::new(), 0.0);
    let mut change_out = (String::new(), 0.0);
    for out in &tx.vout {
        let addr = out
            .script_pub_key
            .address
            .clone()
            .unwrap()
            .assume_checked()
            .to_string();
        if addr == trader_str {
            trader_out = (addr, out.value.to_btc());
        } else {
            change_out = (addr, out.value.to_btc());
        }
    }

    // The wallet's own record of the transaction carries the fee it paid and the
    // height of the block that confirmed it.
    let wallet_tx = miner.get_transaction(&txid, None)?;
    let fee = wallet_tx.fee.unwrap().to_btc();
    let block_height = wallet_tx.info.blockheight.unwrap();

    // out.txt collects the required details, one attribute per line, in the order
    // the grader expects. It lives at the repo root (one level up from ./rust).
    let mut file = File::create("../out.txt")?;
    writeln!(file, "{txid}")?;
    writeln!(file, "{input_address}")?;
    writeln!(file, "{input_amount}")?;
    writeln!(file, "{}", trader_out.0)?;
    writeln!(file, "{}", trader_out.1)?;
    writeln!(file, "{}", change_out.0)?;
    writeln!(file, "{}", change_out.1)?;
    writeln!(file, "{fee}")?;
    writeln!(file, "{block_height}")?;
    writeln!(file, "{block_hash}")?;
    println!("Wrote transaction details to out.txt");

    Ok(())
}
