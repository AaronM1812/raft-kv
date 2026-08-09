use std::collections::HashMap;

//using the file libs, not sure what each specific one does
use std::fs::File;
use std::io::prelude::*;

//using open options in order to use file for reading and writing and create if not exist
use std::fs::OpenOptions;

//some more libraries needed for replaying the log and rebuilding it
use std::io::BufReader;
use std::io::Read;

//this is the hashmap, going to call it storage, we put in struct in rust
struct Storage{
    map: HashMap<String, Vec<u8>>,
    file: File,
}

//these are the methods associated with the storage struct
impl Storage{
    //constructor method which returns a new empty hashmap
    fn new() -> Storage {
        // do work up here
        let file = OpenOptions::new()
                    .read(true)
                    .append(true)
                    .create(true)
                    .open("wal.log")
                    .unwrap();// open or create file

        let mut map = HashMap::new();

        //replay logic
        if let Ok(existing) = File::open("wal.log") {
            let mut reader = BufReader::new(existing);
            // loop goes in here
            loop {
                let mut op_buf = [0u8; 1];
                match reader.read_exact(&mut op_buf) {
                    Ok(_) => { 
                        /* process entry */
                        if op_buf[0] == 0{
                            //extract key and value, not sure yet
                            let mut key_len_buf = [0u8; 4];
                            reader.read_exact(&mut key_len_buf).unwrap();
                            let key_len = u32::from_be_bytes(key_len_buf) as usize;
                            let mut key_buf = vec![0u8; key_len];
                            reader.read_exact(&mut key_buf).unwrap();
                            let key = String::from_utf8(key_buf).unwrap();

                            //for the value
                            let mut val_len_buf = [0u8; 4];
                            reader.read_exact(&mut val_len_buf).unwrap();
                            let val_len = u32::from_be_bytes(val_len_buf) as usize;
                            let mut val_buf = vec![0u8; val_len];
                            reader.read_exact(&mut val_buf).unwrap();
                            let value = val_buf;
                            //then apply onto hashmap
                            map.insert(key.to_string(), value);
                        }
                        else if op_buf[0] == 1{
                            //extract key and value, not sure yet
                            let mut key_len_buf = [0u8; 4];
                            reader.read_exact(&mut key_len_buf).unwrap();
                            let key_len = u32::from_be_bytes(key_len_buf) as usize;
                            let mut key_buf = vec![0u8; key_len];
                            reader.read_exact(&mut key_buf).unwrap();
                            let key = String::from_utf8(key_buf).unwrap();
                            //then apply onto hashmap
                            map.remove(&key);
                        }
                    }
                    Err(_) => break,  // end of file
                }
            }

        }
        // then return the struct
        Storage {
            map,
            file,
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
        //first appending to the WAL
        //so first writing the opcode
        self.file.write_all(&[0u8]).unwrap();
        //then i want to write the length of key
        let key_bytes = key.as_bytes();
        self.file.write_all(&(key_bytes.len() as u32).to_be_bytes()).unwrap();
        //then the key itself
        self.file.write_all(key_bytes).unwrap();
        //and the same again but for value
        let val_bytes = &value;
        self.file.write_all(&(val_bytes.len() as u32).to_be_bytes()).unwrap();
        self.file.write_all(val_bytes).unwrap();

        //now carrying out the operation on the map
        //network layer invokes put and passing in kv
        //will either insert new key with value, or will update if it already exists
        self.map.insert(key.to_string(), value);
    }
    fn delete(&mut self, key: &str){
        //first appending to the WAL
        //so first writing the opcode
        self.file.write_all(&[1u8]).unwrap();
        //then i want to write the length of key
        let key_bytes = key.as_bytes();
        self.file.write_all(&(key_bytes.len() as u32).to_be_bytes()).unwrap();
        //then the key itself
        self.file.write_all(key_bytes).unwrap();
        //will delete if it exists
        self.map.remove(key);
    }
}


//unit tests, carried out in file, integration tests carried out in test folder
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_put_and_get() {
        let _ = std::fs::remove_file("wal.log");  // start clean
        let mut storage = Storage::new();
        storage.put("hello", b"world".to_vec());
        assert_eq!(storage.get("hello"), Some(b"world".to_vec()));
    }

    #[test]
    fn test_delete() {
        let _ = std::fs::remove_file("wal.log");  // start clean
        let mut storage = Storage::new();
        storage.put("hello", b"world".to_vec());
        storage.delete("hello");
        assert_eq!(storage.get("hello"), None);
    }

    #[test]
    fn test_get_missing_key() {
        let _ = std::fs::remove_file("wal.log");  // start clean
        let storage = Storage::new();
        assert_eq!(storage.get("missing"), None);
    }
}