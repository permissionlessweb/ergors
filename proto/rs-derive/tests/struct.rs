use cosmwasm_std::CosmosMsg;
use rs_derive::CosmwasmExt;

#[derive(Clone, PartialEq, Eq, ::prost::Message, CosmwasmExt)]
#[proto_message(type_url = "/bitsong.fantoken.v1beta1.MsgIssue")]
pub struct MsgIssue {
    #[prost(string, tag = "1")]
    pub symbol: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub max_supply: ::prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub uri: ::prost::alloc::string::String,
    #[prost(string, tag = "5")]
    pub authority: ::prost::alloc::string::String,
    #[prost(string, tag = "6")]
    pub minter: ::prost::alloc::string::String,
}

fn main() {
    assert_eq!(
        MsgIssue::TYPE_URL,
        "/bitsong.fantoken.v1beta1.MsgIssue"
    );
    let msg = MsgIssue {
        symbol: "BTSG".to_string(),
        name: "BitSong Token".to_string(),
        max_supply: "1000000".to_string(),
        uri: "https://example.com/fantoken".to_string(),
        authority: "bitsong1authority".to_string(),
        minter: "bitsong1minter".to_string(),
    };

    let _: CosmosMsg = msg.into();
}

mod shim {
    pub struct Any {
        pub type_url: String,
        pub value: Vec<u8>,
    }
}
