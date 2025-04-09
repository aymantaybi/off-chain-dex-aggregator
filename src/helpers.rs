use std::sync::Arc;

use alloy::{
    eips::BlockId,
    network::AnyNetwork,
    providers::{IpcConnect, ProviderBuilder, RootProvider, WsConnect},
    pubsub::PubSubFrontend,
};
use revm::{
    db::{AlloyDB, CacheDB},
    primitives::address,
    Evm,
};
use revm_proxy_db::ProxyDB;
use tracing::info;

pub type AlloyProxyCacheDB = CacheDB<
    ProxyDB<AlloyDB<PubSubFrontend, AnyNetwork, Arc<RootProvider<PubSubFrontend, AnyNetwork>>>>,
>;

pub type AlloyProxyDB =
    ProxyDB<AlloyDB<PubSubFrontend, AnyNetwork, Arc<RootProvider<PubSubFrontend, AnyNetwork>>>>;

pub async fn build_provider() -> eyre::Result<Arc<RootProvider<PubSubFrontend, AnyNetwork>>> {
    /* let connect = IpcConnect::new("/mnt/combined_volume/ronin/chaindata/data/geth.ipc".to_string());
    let provider = ProviderBuilder::new()
        .network::<AnyNetwork>()
        .on_ipc(connect)
        .await?; */

    let endpoint = "wss://ronin-mainnet.core.chainstack.com/4c038c1eed9a64ceae433187dedb22a9";

    let connect = WsConnect::new(endpoint.to_string());
    let provider = ProviderBuilder::new()
        .network::<AnyNetwork>()
        .on_ws(connect)
        .await?;

    let provider = Arc::new(provider);

    Ok(provider)
}

pub async fn build_evm(
    provider: Arc<RootProvider<PubSubFrontend, AnyNetwork>>,
) -> eyre::Result<Evm<'static, (), AlloyProxyCacheDB>> {
    let alloy_db = AlloyDB::new(provider.clone(), BlockId::default()).unwrap();

    let proxy_db = ProxyDB::new(alloy_db);

    let cache_db = CacheDB::new(proxy_db);
    let evm = Evm::builder()
        .with_db(cache_db)
        .modify_cfg_env(|cfg| {
            cfg.chain_id = 2020;
        })
        .modify_tx_env(|tx| {
            tx.caller = address!("c1eb47de5d549d45a871e32d9d082e7ac5d2e3ed");
        })
        .build();

    Ok(evm)
}
