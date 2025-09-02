use std::io::Result;

fn main() -> Result<()> {
    // Compile the proto file using default OUT_DIR
    prost_build::compile_protos(&["spawn.proto"], &["."])?;
    
    println!("cargo:rerun-if-changed=spawn.proto");
    
    Ok(())
}
