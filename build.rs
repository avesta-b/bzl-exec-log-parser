use std::io::Result;

fn main() -> Result<()> {
    // Configure prost to generate basic protobuf support
    let mut config = prost_build::Config::new();
    config.compile_protos(&["spawn.proto"], &["."])?;
    
    println!("cargo:rerun-if-changed=spawn.proto");
    
    Ok(())
}
