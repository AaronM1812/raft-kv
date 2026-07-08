use std::collections::HashMap;

//this is the hashmap, going to call it storage, we put in struct in rust
struct Storage{
    map: HashMap<String, Vec<u8>>,
}

//these are the methods associated with the storage struct
impl Storage{
    //constructor method which returns a new empty hashmap
    fn new() -> Storage{
        Storage{
            map: HashMap::new(),
        }
    }
    //returning option, we might get a value or none
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        //network layer invokes get and passing in k
        //will either get key if it exists, or return error if it doesnt
        //cloned here before get returns a reference not the actual vector
        return self.map.get(key).cloned()
    }
    //these two do not return anything
    fn put(&mut self, key: &str, value: Vec<u8>){
        //network layer invokes put and passing in kv
        //will either insert new key with value, or will update if it already exists
        self.map.insert(key.to_string(), value);
    }
    fn delete(&mut self, key: &str){
        //network layer invokes delete and passing in k
        //will delete if it exists
        self.map.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_put_and_get() {
        let mut storage = Storage::new();
        storage.put("hello", b"world".to_vec());
        assert_eq!(storage.get("hello"), Some(b"world".to_vec()));
    }

    #[test]
    fn test_delete() {
        let mut storage = Storage::new();
        storage.put("hello", b"world".to_vec());
        storage.delete("hello");
        assert_eq!(storage.get("hello"), None);
    }

    #[test]
    fn test_get_missing_key() {
        let storage = Storage::new();
        assert_eq!(storage.get("missing"), None);
    }
}