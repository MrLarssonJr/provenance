# Provenance
A Rust library that provides containers that upon insertion generate a key.
This key will only work with the map that generated it.

## Using the library
Add the following to your `Cargo.toml`:
```toml
[dependencies]
provenance = "0.1.0"
```

### Example
```rust
use provenance::ProvenanceMap;

fn main() {
    let ages = ProvenanceMap::<u32>::new().unwrap();
    let names = ProvenanceMap::<String>::new().unwrap();
    
    let middle_age: Key<u32> = ages.insert(40); // Key generated on insert
    assert_eq!(&40, ages.get(middle_age)); // Key is used to retrieve stored value
    
    // names.get(middle_age); // Compile error, key can only be used with it's map
}
```