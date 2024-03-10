use anyhow::{bail, Result};
use k8s_openapi::api::{
    apps::v1::{Deployment, DeploymentSpec},
    core::v1::{
        Namespace, NodeSelector, NodeSelectorRequirement, NodeSelectorTerm, PersistentVolume,
        PersistentVolumeClaim, Secret, Service,
    },
};
use kube::{api::ObjectMeta, Api};

struct KwpmClient {
    client: kube::Client,
    pv_base_path: String,
}

impl KwpmClient {
    pub async fn new(pv_base_path: impl ToString) -> Result<Self> {
        let client = kube::Client::try_default().await?;
        Ok(Self {
            client,
            pv_base_path: pv_base_path.to_string(),
        })
    }

    pub async fn get_namespaces(&self) -> Result<Vec<Namespace>> {
        let namespaces: Api<Namespace> = Api::all(self.client.clone());
        let ns_list = namespaces.list(&Default::default()).await?;
        Ok(ns_list.items)
    }

    pub async fn get_kwpm_namespaces(&self) -> Result<Vec<Namespace>> {
        Ok(self
            .get_namespaces()
            .await?
            .into_iter()
            .filter(|ns| {
                ns.metadata
                    .name
                    .as_ref()
                    .unwrap_or(&"".to_string())
                    .starts_with("kwpm-")
            })
            .collect())
    }

    pub async fn is_mariadb_created(&self) -> Result<bool> {
        let kwpm_namespaces = self.get_kwpm_namespaces().await?;
        Ok(kwpm_namespaces.iter().any(|ns| {
            ns.metadata
                .name
                .as_ref()
                .unwrap_or(&"".to_string())
                .ends_with("-mariadb")
        }))
    }

    pub async fn create_mariadb_if_not_exists(
        &self,
        mysql_root_password: &str,
        node_hostname: &str,
    ) -> Result<()> {
        if self.is_mariadb_created().await? {
            bail!("MariaDB deployment already exists")
        }

        let ns_name = "kwpm-mariadb";

        let namespace: Namespace = Namespace {
            metadata: ObjectMeta {
                name: Some(ns_name.to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        let deployment: Deployment = serde_yaml::from_str(include_str!(
            "../../kubernetes/mariadb/mariadb-deployment.yaml"
        ))?;
        let mut pv: PersistentVolume =
            serde_yaml::from_str(include_str!("../../kubernetes/mariadb/mariadb-pv.yaml"))?;

        if let Some(pv_spec) = pv.spec.as_mut() {
            if let Some(local) = pv_spec.local.as_mut() {
                local.path = format!("{}/mariadb", self.pv_base_path);
            }

            pv_spec.node_affinity = Some(k8s_openapi::api::core::v1::VolumeNodeAffinity {
                required: Some(NodeSelector {
                    node_selector_terms: vec![NodeSelectorTerm {
                        match_expressions: Some(vec![NodeSelectorRequirement {
                            key: "kubernetes.io/hostname".to_string(),
                            operator: "In".to_string(),
                            values: Some(vec![node_hostname.to_string()]),
                        }]),
                        ..Default::default()
                    }],
                }),
            });
        }

        let pvc: PersistentVolumeClaim =
            serde_yaml::from_str(include_str!("../../kubernetes/mariadb/mariadb-pvc.yaml"))?;
        let svc: Service =
            serde_yaml::from_str(include_str!("../../kubernetes/mariadb/mariadb-svc.yaml"))?;

        let secret = Secret {
            metadata: ObjectMeta {
                name: Some("mysql-pass".to_string()),
                ..Default::default()
            },
            string_data: Some(
                [("password".to_string(), mysql_root_password.to_string())]
                    .iter()
                    .cloned()
                    .collect(),
            ),
            ..Default::default()
        };

        let namespace_api: Api<Namespace> = Api::all(self.client.clone());
        let deployment_api: Api<Deployment> = Api::namespaced(self.client.clone(), ns_name);
        let pv_api: Api<PersistentVolume> = Api::all(self.client.clone());
        let pvc_api: Api<PersistentVolumeClaim> = Api::namespaced(self.client.clone(), ns_name);
        let svc_api: Api<Service> = Api::namespaced(self.client.clone(), ns_name);

        let secret_api: Api<Secret> = Api::namespaced(self.client.clone(), ns_name);

        namespace_api
            .create(&Default::default(), &namespace)
            .await?;
        pv_api.create(&Default::default(), &pv).await?;
        pvc_api.create(&Default::default(), &pvc).await?;
        svc_api.create(&Default::default(), &svc).await?;
        secret_api.create(&Default::default(), &secret).await?;
        deployment_api
            .create(&Default::default(), &deployment)
            .await?;

        Ok(())
    }

    pub async fn remove_mariadb(&self) -> Result<()> {
        let pv_name = "kwpm-mariadb-pv";
        let ns_name = "kwpm-mariadb";

        let namespace_api: Api<Namespace> = Api::all(self.client.clone());
        namespace_api.delete(ns_name, &Default::default()).await?;

        let pv_api: Api<PersistentVolume> = Api::all(self.client.clone());
        pv_api.delete(pv_name, &Default::default()).await?;

        Ok(())
    }
}

#[tokio::main]
async fn main() {
    println!("Hello, world!");

    let client = kube::Client::try_default().await.unwrap();
}

#[cfg(test)]
mod tests {
    use gethostname::gethostname;

    use super::*;

    async fn client() -> KwpmClient {
        KwpmClient::new("/data/volumes/kwpm").await.unwrap()
    }

    #[tokio::test]
    async fn test_get_namespaces() {
        let client = client().await;
        let namespaces = client.get_namespaces().await.unwrap();
        assert!(namespaces.len() > 0);
    }

    #[tokio::test]
    async fn test_get_kwpm_namespaces() {
        let client = client().await;
        let _namespaces = client.get_kwpm_namespaces().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_mariadb() {
        let client = client().await;

        if client.is_mariadb_created().await.unwrap() {
            return;
        }

        let hostname = gethostname();

        let mysql_root_password = "password";
        client
            .create_mariadb_if_not_exists(mysql_root_password, hostname.to_str().unwrap())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_remove_mariadb() {
        let client = client().await;

        if !client.is_mariadb_created().await.unwrap() {
            return;
        }

        client.remove_mariadb().await.unwrap();
    }
}
