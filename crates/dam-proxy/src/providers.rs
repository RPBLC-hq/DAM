use crate::ProxyError;

pub(crate) struct ProviderAdapters {
    http: dam_http_adapter::HttpAdapter,
}

impl ProviderAdapters {
    pub(crate) fn new() -> Result<Self, ProxyError> {
        Ok(Self {
            http: dam_http_adapter::HttpAdapter::new()
                .map_err(|error| ProxyError::ProviderInit(error.to_string()))?,
        })
    }

    pub(crate) fn http(&self) -> &dam_http_adapter::HttpAdapter {
        &self.http
    }
}
