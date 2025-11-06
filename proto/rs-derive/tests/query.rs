use cosmwasm_std::{Empty, QueryRequest};
use rs_derive::CosmwasmExt;

#[derive(
    Clone, PartialEq, Eq, ::prost::Message, serde::Serialize, serde::Deserialize, CosmwasmExt,
)]
#[proto_message(type_url = "/bitsong.fantoken.v1beta1.QueryFanTokenRequest")]
#[proto_query(
    path = "/bitsong.fantoken.v1beta1.Query/FanToken",
    response_type = QueryFanTokenResponse
)]
pub struct QueryFanTokenRequest {
    #[prost(string, tag = "1")]
    pub denom: ::prost::alloc::string::String,
}

#[derive(
    Clone, PartialEq, Eq, ::prost::Message, serde::Serialize, serde::Deserialize, CosmwasmExt,
)]
#[proto_message(type_url = "/bitsong.fantoken.v1beta1.QueryFanTokenResponse")]
pub struct QueryFanTokenResponse {
    #[prost(message, optional, tag = "1")]
    pub fantoken: ::core::option::Option<FanToken>,
}

#[derive(
    Clone, PartialEq, Eq, ::prost::Message, serde::Serialize, serde::Deserialize, CosmwasmExt,
)]
#[proto_message(type_url = "/bitsong.fantoken.v1beta1.FanToken")]
pub struct FanToken {
    #[prost(string, tag = "1")]
    pub denom: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub max_supply: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub minter: ::prost::alloc::string::String,
}

fn main() {
    let _: QueryRequest<Empty> = QueryFanTokenRequest {
        denom: "ufantoken".to_string(),
    }
    .into();
}

mod shim {
    pub struct Any {
        pub type_url: String,
        pub value: Vec<u8>,
    }
}
