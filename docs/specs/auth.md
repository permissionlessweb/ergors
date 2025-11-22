# keys are crucial

- tx required to be signed by user
- user tx required to by signed by node
- nodes require encrypted keys in storage layer
- nodes need to curate their own identities for discoveries/commitments
- external apis require them in headers

## key management is just as crucial

- users need to be able to decide how to store keys during use
- nodes may need to share keys during bootstrapping

## goal: implement encrypted api key layer in storage for node

- during node registration, encrypt and store all providers api keys set in the environment variables
- ensure codebase is updated with new method for access and decrypting llm provider api key via prompts or actions
- define in specification the layer dedicated to the encrypted storage keys 
