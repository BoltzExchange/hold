use cln_plugin::options;

pub const OPTION_DATABASE: options::DefaultStringConfigOption =
    options::ConfigOption::new_str_with_default(
        "hold-database",
        "sqlite://./hold/hold.sqlite",
        "hold database",
    );

pub const OPTION_MPP_TIMEOUT: options::DefaultIntegerConfigOption =
    options::ConfigOption::new_i64_with_default(
        "hold-mpp-timeout",
        60,
        "hold MPP timeout in seconds",
    );

pub const OPTION_GRPC_HOST: options::DefaultStringConfigOption =
    options::ConfigOption::new_str_with_default("hold-grpc-host", "127.0.0.1", "hold gRPC host");

pub const OPTION_GRPC_PORT: options::DefaultIntegerConfigOption =
    options::ConfigOption::new_i64_with_default("hold-grpc-port", 9292, "hold gRPC post");
