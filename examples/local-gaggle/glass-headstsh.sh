#!/bin/sh 
# deploys verifiable wavs service with key that will that will be the instantiator of cw-zeadstash. 
# cw-zeadstash is the smart contract that operates as:
# - ibc-wasm-client 
# - custom x/smart-account authenticator proxy for proof verification & extensible on-chain services.
#   here, we use our custom authenticator middleware for granulatrity in smart contract composition possibilities.
#
# 
