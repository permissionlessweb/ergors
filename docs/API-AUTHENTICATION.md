# API Authentication Guide

This guide explains how to make authenticated calls to CW-HO permissioned endpoints using Ed25519 cryptographic signatures.

## Overview

CW-HO uses Ed25519 signature-based authentication to protect sensitive endpoints. Public endpoints (like `/health`) are accessible without authentication, while protected endpoints require valid cryptographic signatures in request headers.

## Authentication Requirements

Protected endpoints require three headers:

- `x-signature`: Ed25519 signature (hex-encoded)
- `x-timestamp`: Unix timestamp in seconds
- `x-public-key`: Ed25519 public key (hex-encoded)

### Signature Generation

The signature is computed over a Blake3 hash of the request body concatenated with the timestamp:

```
signature = sign(Blake3(body || timestamp))
```

Where:

- `body` is the raw request body bytes (empty for GET requests)
- `timestamp` is the Unix timestamp as a string
- `||` represents concatenation

**Example:**

```
body = '{"target_node":"user@192.168.1.100"}'
timestamp = "1699564800"
message = Blake3(body_bytes || timestamp_bytes)
signature = Ed25519Sign(private_key, message)
```

## Public Endpoint (No Authentication)

### cURL

```bash
curl -X GET http://localhost:8080/health
```

### Response

```json
{
  "status": "ok",
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "storage_status": "healthy",
  "network_status": "connected (3 peers)"
}
```

## Protected Endpoint (With Authentication)

### Step 1: Generate Signature

The message to sign follows the format: `{METHOD}:{PATH}:{TIMESTAMP}`

### cURL Example

```bash
#!/bin/bash

# Your Ed25519 private key (hex-encoded)
PRIVATE_KEY="your_private_key_hex"
PUBLIC_KEY="your_public_key_hex"

# Request details
METHOD="POST"
PATH="/orchestrate/bootstrap"
TIMESTAMP=$(date +%s)

# Create message to sign
# Sign the message (using a signing tool)


# Make authenticated request
curl -X POST http://localhost:8080/orchestrate/bootstrap \
  -H "Content-Type: application/json" \
  -H "x-signature: $SIGNATURE" \
  -H "x-timestamp: $TIMESTAMP" \
  -H "x-public-key: $PUBLIC_KEY" \
  -d '{
    "target_node": "user@192.168.1.100",
    "ssh_user": "ubuntu",
    "ssh_port": "22"
  }'
```

## Language-Specific Examples

### Rust

```rust
use blake3;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use hex;
use reqwest;
use serde_json::json;

async fn make_authenticated_request() -> Result<(), Box<dyn std::error::Error>> {
    // Your Ed25519 key pair
    let signing_key = SigningKey::from_bytes(&[/* your 32-byte private key */]);
    let verifying_key = VerifyingKey::from(&signing_key);

    // Request body
    let body = json!({
        "target_node": "user@192.168.1.100",
        "ssh_user": "ubuntu",
        "ssh_port": "22"
    });
    let body_bytes = serde_json::to_vec(&body)?;

    // Timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .to_string();

    // Create message to sign: Blake3(body || timestamp)
    let mut contents = Vec::new();
    contents.extend_from_slice(&body_bytes);
    contents.extend_from_slice(timestamp.as_bytes());
    let message = blake3::hash(&contents);

    // Sign the message
    let signature = signing_key.sign(message.as_bytes());

    // Make request
    let client = reqwest::Client::new();
    let response = client
        .post("http://localhost:8080/orchestrate/bootstrap")
        .header("x-signature", hex::encode(signature.to_bytes()))
        .header("x-timestamp", timestamp)
        .header("x-public-key", hex::encode(verifying_key.to_bytes()))
        .body(body_bytes)
        .header("content-type", "application/json")
        .send()
        .await?;

    println!("Response: {:?}", response.text().await?);
    Ok(())
}
```

### JavaScript/TypeScript (Node.js)

```javascript
const nacl = require('tweetnacl');
const axios = require('axios');

async function makeAuthenticatedRequest() {
    // Your Ed25519 key pair
    const privateKey = Buffer.from('your_private_key_hex', 'hex');
    const publicKey = Buffer.from('your_public_key_hex', 'hex');
    const keyPair = nacl.sign.keyPair.fromSeed(privateKey);

    // Request details
    // Create message to sign
    // Sign the message
 
    // Make request
    const response = await axios.post(
        `http://localhost:8080${path}`,
        {
            target_node: 'user@192.168.1.100',
            ssh_user: 'ubuntu',
            ssh_port: '22'
        },
        {
            headers: {
                'x-signature': Buffer.from(signature).toString('hex'),
                'x-timestamp': timestamp,
                'x-public-key': publicKey.toString('hex')
            }
        }
    );

    console.log('Response:', response.data);
}
```

### Python

```python
import time
import requests
from nacl.signing import SigningKey
from nacl.encoding import HexEncoder

def make_authenticated_request():
    # Your Ed25519 key pair
    signing_key = SigningKey(bytes.fromhex('your_private_key_hex'))
    verify_key = signing_key.verify_key

    # Request details
    method = 'POST'
    path = '/orchestrate/bootstrap'
    timestamp = str(int(time.time()))

    # Create message to sign
    message = f"{method}:{path}:{timestamp}"

    # Sign the message
    signed = signing_key.sign(message.encode('utf-8'))
    signature = signed.signature.hex()

    # Make request
    response = requests.post(
        f'http://localhost:8080{path}',
        headers={
            'x-signature': signature,
            'x-timestamp': timestamp,
            'x-public-key': verify_key.encode(encoder=HexEncoder).decode('utf-8')
        },
        json={
            'target_node': 'user@192.168.1.100',
            'ssh_user': 'ubuntu',
            'ssh_port': '22'
        }
    )

    print('Response:', response.json())

if __name__ == '__main__':
    make_authenticated_request()
```

## gRPC Example

For gRPC, metadata is used instead of HTTP headers:

```rust
use tonic::{Request, metadata::MetadataValue};

async fn grpc_authenticated_request() -> Result<(), Box<dyn std::error::Error>> {
    // Sign message as before
    let signature_hex = "...";
    let timestamp = "...";
    let public_key_hex = "...";

    // Create gRPC request with metadata

    // Add auth metadata
    request.metadata_mut().insert(
        "x-signature",
        MetadataValue::from_str(&signature_hex)?
    );
    request.metadata_mut().insert(
        "x-timestamp",
        MetadataValue::from_str(&timestamp)?
    );
    request.metadata_mut().insert(
        "x-public-key",
        MetadataValue::from_str(&public_key_hex)?
    );

    // Make gRPC call
    // let response = client.bootstrap_node(request).await?;

    Ok(())
}
```

## JSON-RPC Example

```bash
curl -X POST http://localhost:8080/rpc \
  -H "Content-Type: application/json" \
  -H "x-signature: $SIGNATURE" \
  -H "x-timestamp: $TIMESTAMP" \
  -H "x-public-key: $PUBLIC_KEY" \
  -d '{
    "jsonrpc": "2.0",
    "method": "orchestrate.bootstrap",
    "params": {
      "target_node": "user@192.168.1.100",
      "ssh_user": "ubuntu",
      "ssh_port": "22"
    },
    "id": 1
  }'
```

## Available Endpoints

### Public Endpoints (No Authentication)

- `GET /health` - Server health status

### Protected Endpoints (Authentication Required)

- `GET /api/prompts` - Query stored prompts
- `POST /api/prompt` - Submit a prompt for processing
- `POST /orchestrate/bootstrap` - Bootstrap a new node
- `POST /orchestrate/fractal` - Create fractal HOE
- `POST /orchestrate/prune` - Prune node state
- `GET /network/topology` - Get network topology

## Security Considerations

1. **Timestamp Validation**: Requests with timestamps older than 5 minutes are rejected to prevent replay attacks.

2. **Private Key Security**: Never share or commit your private key. Store it securely (e.g., environment variables, secret management systems).

3. **HTTPS in Production**: Always use HTTPS in production to prevent man-in-the-middle attacks.

4. **Key Rotation**: Implement key rotation policies for long-running systems.

## Error Responses

### Missing Headers

```json
{
  "error": "Missing signature header"
}
```

**HTTP Status**: 401 Unauthorized

### Invalid Signature

```json
{
  "error": "Signature verification failed"
}
```

**HTTP Status**: 403 Forbidden

### Expired Request

```json
{
  "error": "Request expired"
}
```

## Proto-based Type/Value Pattern

CW-HO follows a proto-based type/value tuple pattern for all request/response messages. Each endpoint expects:

- **Request**: Proto-generated message type (e.g., `BootstrapNodeRequest`)
- **Response**: Proto-generated message type (e.g., `BootstrapNodeResponse`)

This ensures type safety and enables versioning control across the API surface.

## Key Generation

---

For more information on the CW-HO architecture and API design, see the [main README](../README.md).
