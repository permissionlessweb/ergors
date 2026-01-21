/**
 * gRPC Client Library for Ergors Management Service
 *
 * Provides connection management and authentication helpers for OpenCode tools
 * to communicate with the ergors gRPC server.
 */

import { ChannelCredentials, Metadata } from "@grpc/grpc-js";

// Configuration from environment
const GRPC_ADDR = process.env.ERGORS_GRPC_ADDR || "localhost:50051";
const AUTH_TOKEN = process.env.ERGORS_AUTH_TOKEN;

// Lazy client instance
let client: any | null = null;

/**
 * Create or return existing gRPC client connection
 */
export async function createGrpcClient(): Promise<any> {
  if (!client) {
    // Dynamic import to handle generated types
    // TODO: Replace with actual generated client once proto types are generated
    const { ManagementServiceClient } = await import("./gen/management_grpc_pb");
    client = new ManagementServiceClient(
      GRPC_ADDR,
      ChannelCredentials.createInsecure()
    );
  }
  return client;
}

/**
 * Create metadata with Bearer token authentication
 */
export function createAuthMetadata(): Metadata {
  const metadata = new Metadata();
  if (AUTH_TOKEN) {
    metadata.set("authorization", `Bearer ${AUTH_TOKEN}`);
  }
  return metadata;
}

/**
 * Wrap a request with authentication metadata
 */
export function withAuth<T>(request: T): { request: T; metadata: Metadata } {
  return {
    request,
    metadata: createAuthMetadata(),
  };
}

/**
 * Execute a gRPC call with automatic error handling
 */
export async function grpcCall<TReq, TRes>(
  method: (req: TReq, metadata: Metadata) => Promise<TRes>,
  request: TReq
): Promise<TRes> {
  const metadata = createAuthMetadata();
  try {
    return await method(request, metadata);
  } catch (error: any) {
    throw new Error(`gRPC call failed: ${error.message || error}`);
  }
}

// Export configuration for debugging
export const config = {
  grpcAddr: GRPC_ADDR,
  hasAuthToken: !!AUTH_TOKEN,
};
