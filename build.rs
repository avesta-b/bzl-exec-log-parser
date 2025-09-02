use std::io::Result;

fn main() -> Result<()> {
    // Configure prost to generate basic protobuf support
    let mut config = prost_build::Config::new();
    
    // Add serde support for JSON serialization
    config.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    config.type_attribute(".", "#[serde(rename_all = \"camelCase\")]");
    
    config.compile_protos(&["spawn.proto"], &["."])?;
    
    println!("cargo:rerun-if-changed=spawn.proto");
    
    Ok(())
}
