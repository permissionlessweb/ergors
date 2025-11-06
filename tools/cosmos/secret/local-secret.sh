#!/bin/sh

#1. download localsecret & relayer if we need 
docker pull ghcr.io/scrtlabs/localsecret
# 2. spin up environment
docker run -it -p 9091:9091 -p 26657:26657 -p 1317:1317 -p 5000:5000 \
  --name localsecret ghcr.io/scrtlabs/localsecret

# 3. Call faucet for all accounts http://localhost:5000
# ex: curl "http://localhost:5000/faucet?address=${ADDRESS}"

# 4. create alias for executing commands: docker exec -it localsecret secretcli [command]

# 5. configure local cli binary: 
secretcli config chain-id secretdev-1
secretcli config node http://localhost:26657
secretcli config output json

SGX_MODE=SW secretcli status