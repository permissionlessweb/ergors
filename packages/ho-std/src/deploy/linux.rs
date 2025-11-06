
// resuable scripts for lifecycle of deploying a new node to a given linux workstation. 
// Currently we use ssh keys and scp to get the node binary and config over to a fresh instance.



// 1. connect via ssh 
// 2. transfer bundle of resources over to node
// 3. execute bash script on remote host to install minimum prerequisites
// 4. start node