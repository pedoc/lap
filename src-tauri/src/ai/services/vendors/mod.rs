use super::{
    client::{EmbeddingPurpose, InputPart, VendorRequest},
    config::{EmbeddingProtocol, Resolved},
};

mod ark;
mod dashscope;
mod siliconflow;

pub(super) fn build(
    model: &Resolved,
    parts: &[InputPart],
    purpose: EmbeddingPurpose,
) -> Result<VendorRequest, String> {
    match model.provider.embedding_protocol {
        EmbeddingProtocol::DashscopeMultimodal => {
            dashscope::build(model, parts, purpose.is_query())
        }
        EmbeddingProtocol::ArkMultimodal => ark::build(model, parts, purpose),
        EmbeddingProtocol::SiliconflowMultimodal => {
            siliconflow::build(model, parts, purpose.is_query())
        }
        _ => Err("未配置原生多模态 Embedding 协议".into()),
    }
}
