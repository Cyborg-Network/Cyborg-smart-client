use serde_json::Value;
use std::{
    fs, 
    thread, 
    process::{Command, Stdio}, 
    sync::{Arc, Mutex}, 
    path::PathBuf, 
    io::{BufRead, BufReader}
};
use home::home_dir;
use anyhow::{Context, Result}; // Importing anyhow for better error handling

pub type LogsStorage = Arc<Mutex<Vec<String>>>;

// Read JSON map from a file and return the Value or an error
fn read_json_map(file_path: PathBuf) -> Result<Value> {
    let data = fs::read_to_string(&file_path)
        .with_context(|| format!("Failed to read file: {:?}", file_path))?;
    
    let json_map: Value = serde_json::from_str(&data)
        .with_context(|| "Failed to parse JSON data")?;
    
    Ok(json_map)
}

// Fetch the deployment name from the JSON map based on input
fn get_deployment_name_from_json_map(input: &str, json_map: &Value) -> Option<String> {
    if let Value::Object(map) = json_map {
        if let Some(value) = map.get(input) {
            if let Some(deployment_name) = value.as_str() {
                return Some(deployment_name.to_string());
            }
        }
    }
    None
}

pub fn aggregate_new_logs(logs_storage: LogsStorage) -> Result<(), Box<dyn std::error::Error>> {

    let home_path = home_dir().context("Failed to get home directory")?;
    let file_path = home_path.join("Worker/deploymentsMap.json");
    let json_map = read_json_map(file_path)
        .context("Failed to read or parse the deploymentsMap.json file")?;
    
    let deployment_name = get_deployment_name_from_json_map("deployment", &json_map)
        .context("Failed to get deployment name from JSON map")?;

    let mut process = Command::new("sudo")
        .arg("kubectl")
        .arg("logs")
        .arg("-l")
        .arg(format!("app={}", deployment_name))
        .arg("--all-containers=true")
        .arg("--follow") 
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to execute `kubectl logs -f` for deployment: {}", deployment_name))?;



    let stdout = process.stdout.take().expect("Failed to capture stdout");
    let reader = BufReader::new(stdout);

    for line in reader.lines() {
        if let Ok(log_line) = line {
            let mut logs = logs_storage.lock().unwrap();
            logs.push(log_line);
        }
    }

    Ok(())
}

pub fn retrieve_new_logs(logs_storage: LogsStorage) -> Vec<String> {
    let mut logs = logs_storage.lock().unwrap();

    let new_logs = logs.clone();
    logs.clear();

    new_logs
}