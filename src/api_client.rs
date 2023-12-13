// methods

use subxt::ClientBuilder;

let api = ClientBuilder::new()
    .build()
    .await?
    .to_runtime_api::<RuntimeApi>();

// fail early if metadata is outdated
api.validate_metadata()?;

// reading storage
let value = api
    .storage()
    .pallet_edge_connect()
    .connections(..., Some(block_hash))
    .await?;

// events
let block_events = api
    .events()
    .at(block_hash)
    .await?
    .iter()
    collect()::<Vec<_>>();

// event subscritpion
let filter_events = api
    .events()
    .subscribe()
    .await?
    .filter_events::<(runtime::pallet_edge_connect::Event, )>();

while let Some(event) = filter_events.next().await {
    println!("{event:?}");
}

// Submit extrinsics
let signer = PairSigner::new(AccountKeyring::Alice.pair());
let transaction_events = api
    .tx()
    .pallet_edge_connect()
    .connect(...)?
    .sign_and_submit_then_watch_default(&signer)
    .await?
    .wait_for_finalized_success()
    .await?;

// custom RPC
let head = api.client.rpc().finalized_head().await?;
let result = api
    .client
    .rpc()
    .request(
        "edgeConnect_createConnection",
        subxt::rpc::rpc_params![param, some(head)],
    )
    .await?;