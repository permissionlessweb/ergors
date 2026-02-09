# E2E scripts

The goal of these test are 100% local, fully automated e2e testing of engines functionalities, simulating production enviroment use. We have a mnumber of services that are deployed to assit in simulation of this production enviroment, including:

- ethereum localnet spun up via anvil
- akash network localnet, & provider spun up via kind and kube-clusers
- mock inference provider spun up via docker (for testing api-key usage)

We want 0 human intervention in this e2e tests. This is a closed loop verifying itself.
