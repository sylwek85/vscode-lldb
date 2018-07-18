use serde_derive;
use serde_json;
use std::collections::btree_map::BTreeMap;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LaunchConfig {
    pub args: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub env: Option<BTreeMap<String, String>>,
    pub stdio: Option<Vec<String>>,
    pub terminal: Option<Terminal>,
    pub stop_on_entry: Option<bool>,
    pub init_commands: Option<Vec<String>>,
    pub pre_run_commands: Option<Vec<String>>,
    pub post_run_commands: Option<Vec<String>>,
    pub exit_commands: Option<Vec<String>>,
    pub expressions: Option<Expressions>,
    pub source_map: Option<BTreeMap<String, String>>,
    pub source_languages: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AttachConfig {
    pub program: Option<String>,
    pub pid: Option<Pid>,
    pub wait_for: Option<bool>,
    pub stop_on_entry: Option<bool>,
    pub init_commands: Option<Vec<String>>,
    pub pre_run_commands: Option<Vec<String>>,
    pub post_run_commands: Option<Vec<String>>,
    pub exit_commands: Option<Vec<String>>,
    pub expressions: Option<Expressions>,
    pub source_map: Option<BTreeMap<String, String>>,
    pub source_languages: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CustomConfig {}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum Pid {
    Number(u32),
    String(String),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
#[serde(rename_all = "camelCase")]
pub enum Terminal {
    Integrated,
    External,
    Console,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
#[serde(rename_all = "camelCase")]
pub enum Expressions {
    Simple,
    Python,
    Native,
}
