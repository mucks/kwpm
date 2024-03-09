# K8S Wordpress Manager

## Description
This application let's you easily manage multiple wordpress instances on a kubernetes cluster.

## Requirements
* A kubernetes cluster
* nginx-ingress-controller installed on the cluster

## Notes

* when ufw is enabled it requires `sudo ufw allow in on cali+` && `sudo ufw allow out on cali+` to allow calico to work properly
* 