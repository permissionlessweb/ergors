//! Re-exports of generated Akash protocol buffer types
//!
//! This module provides convenient access to all generated proto types
//! used in the Akash deployment workflow.

pub use ergors_proto::ergors::akash::deployment::v1beta3::*;

// Re-export commonly used types
pub type Deployment = ergors_proto::ergors::akash::deployment::v1beta3::Deployment;
pub type DeploymentId = ergors_proto::ergors::akash::deployment::v1beta3::DeploymentId;
pub type Group = ergors_proto::ergors::akash::deployment::v1beta3::Group;
pub type GroupId = ergors_proto::ergors::akash::deployment::v1beta3::GroupId;
pub type GroupSpec = ergors_proto::ergors::akash::deployment::v1beta3::GroupSpec;

// Message types for transactions
pub type MsgCreateDeployment =
    ergors_proto::ergors::akash::deployment::v1beta3::MsgCreateDeployment;
pub type MsgUpdateDeployment =
    ergors_proto::ergors::akash::deployment::v1beta3::MsgUpdateDeployment;
pub type MsgCloseDeployment = ergors_proto::ergors::akash::deployment::v1beta3::MsgCloseDeployment;

// Query types
pub type QueryDeploymentRequest =
    ergors_proto::ergors::akash::deployment::v1beta3::QueryDeploymentRequest;
pub type QueryDeploymentResponse =
    ergors_proto::ergors::akash::deployment::v1beta3::QueryDeploymentResponse;
pub type QueryDeploymentsRequest =
    ergors_proto::ergors::akash::deployment::v1beta3::QueryDeploymentsRequest;
pub type QueryDeploymentsResponse =
    ergors_proto::ergors::akash::deployment::v1beta3::QueryDeploymentsResponse;

// TODO: Add re-exports for market, cert, and provider services as they are implemented
