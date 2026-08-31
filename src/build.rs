#[cfg(not(any(feature = "dev", feature = "prod")))]
compile_error!("one of 'dev' or 'prod' features must be enabled");
#[cfg(all(feature = "dev", feature = "prod"))]
compile_error!("the 'dev' and 'prod' features are mutually exclusive");

#[cfg(feature = "dev")]
pub const BUILD_ID: &str = "Dev";
#[cfg(feature = "prod")]
pub const BUILD_ID: &str = "Prod";

#[cfg(feature = "dev")]
pub const HOST_URL: &str = "http://localhost:80";
#[cfg(feature = "prod")]
pub const HOST_URL: &str = "https://backend.on.vaultnite.com";
