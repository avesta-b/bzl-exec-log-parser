mod proto;

use proto::*;

fn main() {
    println!("Hello, world!");
    println!("Protobuf bindings are ready to use!");
    
    // Example: Create a new Digest message
    let digest = Digest {
        hash: "abc123".to_string(),
        size_bytes: 1024,
        hash_function_name: "SHA256".to_string(),
    };
    
    println!("Created digest: {:?}", digest);
}
