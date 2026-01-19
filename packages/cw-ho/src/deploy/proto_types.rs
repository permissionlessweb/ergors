//! Re-exports of generated Akash protocol buffer types
//!
//! This module provides convenient access to all generated proto types
//! used in the Akash deployment workflow.

pub use ho_std::types::ergors::akash::deployment::v1beta3::*;

// Re-export commonly used types
pub type Deployment = ho_std::types::ergors::akash::deployment::v1beta3::Deployment;
pub type DeploymentId = ho_std::types::ergors::akash::deployment::v1beta3::DeploymentId;
pub type Group = ho_std::types::ergors::akash::deployment::v1beta3::Group;
pub type GroupId = ho_std::types::ergors::akash::deployment::v1beta3::GroupId;
pub type GroupSpec = ho_std::types::ergors::akash::deployment::v1beta3::GroupSpec;

// Message types for transactions
pub type MsgCreateDeployment =
    ho_std::types::ergors::akash::deployment::v1beta3::MsgCreateDeployment;
pub type MsgUpdateDeployment =
    ho_std::types::ergors::akash::deployment::v1beta3::MsgUpdateDeployment;
pub type MsgCloseDeployment = ho_std::types::ergors::akash::deployment::v1beta3::MsgCloseDeployment;

// Query types
pub type QueryDeploymentRequest =
    ho_std::types::ergors::akash::deployment::v1beta3::QueryDeploymentRequest;
pub type QueryDeploymentResponse =
    ho_std::types::ergors::akash::deployment::v1beta3::QueryDeploymentResponse;
pub type QueryDeploymentsRequest =
    ho_std::types::ergors::akash::deployment::v1beta3::QueryDeploymentsRequest;
pub type QueryDeploymentsResponse =
    ho_std::types::ergors::akash::deployment::v1beta3::QueryDeploymentsResponse;

// TODO: Add re-exports for market, cert, and provider services as they are implemented
