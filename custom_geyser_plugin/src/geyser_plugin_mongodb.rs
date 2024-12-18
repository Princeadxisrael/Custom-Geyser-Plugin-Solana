/// Main entry for the  plugin
use {
    crate::{
        accounts_selector::AccountsSelector,
        mongodb_client::{ParallelMongodbClient, MongoClientBuilder},
        transaction_selector::TransactionSelector,
    },
    bs58,
    log::*,
    serde_derive::{Deserialize, Serialize},
    serde_json,
    solana_geyser_plugin_interface::geyser_plugin_interface::{
        GeyserPlugin, GeyserPluginError, ReplicaAccountInfoVersions, ReplicaBlockInfoVersions,
        ReplicaTransactionInfoVersions, Result, SlotStatus,
    },
    solana_measure::measure::Measure,
    solana_metrics::*,
    std::{fs::File, io::Read},
    thiserror::Error,
};
#[derive(Default)]
pub struct GeyserPluginMongodb {
    client: Option<ParallelMongodbClient>,
    accounts_selector: Option<AccountsSelector>,
    transaction_selector: Option<TransactionSelector>,
    batch_starting_slot: Option<u64>,
}

impl std::fmt::Debug for GeyserPluginMongodb {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

/// The Configuration for the MongoDb plugin
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GeyserPluginMongoDBConfig {
    /// The host name or IP of the MongoDb server
    pub host: Option<String>,

    /// The user name of the MongoDb server.
    pub user: Option<String>,

    /// The port number of the MongoDb database, the default is 27017
    pub port: Option<u16>,

    /// The connection string of MongoDb database, if this is set
    /// `host`, `user` and `port` will be ignored.
    pub connection_str: Option<String>,

    /// Controls the number of threads establishing connections to
    /// the MongoDb server. The default is 10.
    pub threads: Option<usize>,

    /// Controls the batch size when bulk loading accounts.
    /// The default is 10.
    pub batch_size: Option<usize>,

    /// Controls whether to panic the validator in case of errors
    /// writing to MongoDb server. The default is false
    pub panic_on_db_errors: Option<bool>,

    /// Indicates whether to store historical data for accounts
    pub store_account_historical_data: Option<bool>,

    /// Controls whether to use SSL based connection to the database server.
    /// The default is false
    pub use_ssl: Option<bool>,

    /// Specify the path to MongoDB server's certificate file
    pub server_ca: Option<String>,

    /// Specify the path to the local client's certificate file
    pub client_cert: Option<String>,

    /// Specify the path to the local client's private PEM key file.
    pub client_key: Option<String>,

    /// Controls whether to index the token owners. The default is false
    pub index_token_owner: Option<bool>,

    /// Controls whether to index the token mints. The default is false
    pub index_token_mint: Option<bool>,

    /// Controls if this plugin can read the database on_load() to find heighest slot
    /// and ignore upsetr accounts (at_startup) that should already exist in DB
    #[serde(default)]
    pub skip_upsert_existing_accounts_at_startup: bool,
}

#[derive(Error, Debug)]
pub enum GeyserPluginMongoDbError {
    #[error("Error connecting to the backend data store. Error message: ({msg})")]
    DataStoreConnectionError { msg: String },

    #[error("Error preparing data store schema. Error message: ({msg})")]
    DataSchemaError { msg: String },

    #[error("Error preparing data store schema. Error message: ({msg})")]
    ConfigurationError { msg: String },

    #[error("Replica account V0.0.1 not supported anymore")]
    ReplicaAccountV001NotSupported,
}

impl GeyserPlugin for GeyserPluginMongodb{
    
}
