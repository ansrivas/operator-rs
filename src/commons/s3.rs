//! Implementation of the bucket definition as described in
//! the <https://github.com/stackabletech/documentation/pull/177>
//!
//! Operator CRDs are expected to use the [S3BucketDef] as an entry point to this module
//! and obtain an [InlinedS3BucketSpec] by calling [`S3BucketDef::resolve`].
//!
use crate::commons::tls::Tls;
use crate::error;
use crate::{client::Client, error::OperatorResult};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// S3 bucket specification containing only the bucket name and an inlined or
/// referenced connection specification.
#[derive(Clone, CustomResource, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[kube(
    group = "s3.stackable.tech",
    version = "v1alpha1",
    kind = "S3Bucket",
    plural = "s3buckets",
    crates(
        kube_core = "kube::core",
        k8s_openapi = "k8s_openapi",
        schemars = "schemars"
    ),
    namespaced
)]
#[serde(rename_all = "camelCase")]
pub struct S3BucketSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bucket_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionDef>,
}

impl S3BucketSpec {
    /// Convenience function to retrieve the spec of a S3 bucket resource from the K8S API service.
    pub async fn get(
        resource_name: &str,
        client: &Client,
        namespace: Option<&str>,
    ) -> OperatorResult<S3BucketSpec> {
        client
            .get::<S3Bucket>(resource_name, namespace)
            .await
            .map(|crd| crd.spec)
            .map_err(|_source| error::Error::MissingS3Bucket {
                name: resource_name.to_string(),
            })
    }

    /// Map &self to an [InlinedS3BucketSpec] by obtaining connection spec from the K8S API service if necessary
    pub async fn inlined(
        &self,
        client: &Client,
        namespace: Option<&str>,
    ) -> OperatorResult<InlinedS3BucketSpec> {
        match self.connection.as_ref() {
            Some(ConnectionDef::Reference(res_name)) => Ok(InlinedS3BucketSpec {
                connection: Some(S3ConnectionSpec::get(res_name, client, namespace).await?),
                bucket_name: self.bucket_name.clone(),
            }),
            Some(ConnectionDef::Inline(conn_spec)) => Ok(InlinedS3BucketSpec {
                bucket_name: self.bucket_name.clone(),
                connection: Some(conn_spec.clone()),
            }),
            None => Ok(InlinedS3BucketSpec {
                bucket_name: self.bucket_name.clone(),
                connection: None,
            }),
        }
    }
}

/// Convenience struct with the connection spec inlined.
pub struct InlinedS3BucketSpec {
    pub bucket_name: Option<String>,
    pub connection: Option<S3ConnectionSpec>,
}

impl InlinedS3BucketSpec {
    /// Build the endpoint URL from [S3ConnectionSpec::host] and [S3ConnectionSpec::port].
    pub fn endpoint(&self) -> Option<String> {
        match self.connection.as_ref() {
            Some(conn_spec) => conn_spec.host.as_ref().map(|h| match conn_spec.port {
                Some(p) => format!("s3a://{h}:{p}"),
                _ => format!("s3a://{h}"),
            }),
            _ => None,
        }
    }

    /// Shortcut to [S3ConnectionSpec::secret_class]
    pub fn secret_class(&self) -> Option<String> {
        match self.connection.as_ref() {
            Some(conn_spec) => conn_spec.secret_class.clone(),
            _ => None,
        }
    }
}

/// Operators are expected to define fields for this type in order to work with S3 buckets.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum S3BucketDef {
    Inline(S3BucketSpec),
    Reference(String),
}

impl S3BucketDef {
    /// Returns an [InlinedS3BucketSpec].
    pub async fn resolve(
        &self,
        client: &Client,
        namespace: Option<&str>,
    ) -> OperatorResult<InlinedS3BucketSpec> {
        match self {
            S3BucketDef::Inline(s3_bucket) => s3_bucket.inlined(client, namespace).await,
            S3BucketDef::Reference(_s3_bucket) => {
                S3BucketSpec::get(_s3_bucket.as_str(), client, namespace)
                    .await?
                    .inlined(client, namespace)
                    .await
            }
        }
    }
}

/// S3 connection definition used by [S3BucketSpec]
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionDef {
    Inline(S3ConnectionSpec),
    Reference(String),
}

/// S3 connection definition as CRD.
#[derive(CustomResource, Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[kube(
    group = "s3.stackable.tech",
    version = "v1alpha1",
    kind = "S3Connection",
    plural = "s3connections",
    crates(
        kube_core = "kube::core",
        k8s_openapi = "k8s_openapi",
        schemars = "schemars"
    ),
    namespaced
)]
#[serde(rename_all = "camelCase")]
pub struct S3ConnectionSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<Tls>,
}
impl S3ConnectionSpec {
    /// Convenience function to retrieve the spec of a S3 connection resource from the K8S API service.
    pub async fn get(
        resource_name: &str,
        client: &Client,
        namespace: Option<&str>,
    ) -> OperatorResult<S3ConnectionSpec> {
        client
            .get::<S3Connection>(resource_name, namespace)
            .await
            .map(|conn| conn.spec)
            .map_err(|_source| error::Error::MissingS3Connection {
                name: resource_name.to_string(),
            })
    }
}

#[cfg(test)]
mod test {
    use crate::commons::s3::ConnectionDef;
    use crate::commons::s3::{S3BucketSpec, S3ConnectionSpec};

    #[test]
    fn test_ser_inline() {
        let bucket = S3BucketSpec {
            bucket_name: Some("test-bucket-name".to_owned()),
            connection: Some(ConnectionDef::Inline(S3ConnectionSpec {
                host: Some("host".to_owned()),
                port: Some(8080),
                secret_class: None,
                tls: None,
            })),
        };

        assert_eq!(
            serde_yaml::to_string(&bucket).unwrap(),
            "---
bucketName: test-bucket-name
connection:
  inline:
    host: host
    port: 8080
"
            .to_owned()
        )
    }
}
