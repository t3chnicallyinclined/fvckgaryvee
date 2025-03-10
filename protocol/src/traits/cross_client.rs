use crate::types::{Block, BlockNumber, Hash, Log, Proof, SignedTransaction};
use crate::{async_trait, traits::Context, ProtocolResult};

#[async_trait]
pub trait CrossAdapter: Send + Sync {
    async fn watch_ckb_client(&self, ctx: Context) -> ProtocolResult<()>;

    async fn send_axon_tx(&self, ctx: Context, stx: SignedTransaction) -> ProtocolResult<()>;

    async fn send_ckb_tx(&self, ctx: Context) -> ProtocolResult<()>;
}

#[async_trait]
pub trait CrossClient: Send + Sync {
    async fn set_evm_log(
        &self,
        ctx: Context,
        block_number: BlockNumber,
        block_hash: Hash,
        logs: &[Vec<Log>],
    );

    async fn set_checkpoint(&self, ctx: Context, block: Block, proof: Proof);
}
