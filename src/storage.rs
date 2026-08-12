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
pub struct Storage{
    map: HashMap<String, Vec<u8>>,
    file: File,
}

//function returns one entry, but it has 2 shapes, we need enum to express this
enum Entry {
    Put { key: String, value: Vec<u8> },
    Delete { key: String },
}

//these are the methods associated with the storage struct
impl Storage{
    //constructor method which returns a new empty hashmap
    pub fn new(path: &str) -> Storage {
        // do work up here
        let file = OpenOptions::new()
                    .read(true)
                    .append(true)
                    .create(true)
                    .open(path)
                    .unwrap();// open or create file

        let mut map = HashMap::new();

        //replay logic
        if let Ok(existing) = File::open(path) {
            let mut reader = BufReader::new(existing);
            // loop goes in here
            loop {
                match read_entry(&mut reader) {
                    Ok(Entry::Put { key, value }) => { map.insert(key, value); }
                    Ok(Entry::Delete { key }) => { map.remove(&key); }
                    Err(_) => break,
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
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        //network layer invokes get and passing in k
        //will either get key if it exists, or return error if it doesnt
        //cloned here before get returns a reference not the actual vector
        return self.map.get(key).cloned()
    }
    //these two do not return anything
    pub fn put(&mut self, key: &str, value: Vec<u8>){
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
        //this is needed to sync ALL, including meta data for read back
        //to the file, instead of letting OS do it when it wants
        //may cause failures when crashing
        self.file.sync_all().unwrap();

        //now carrying out the operation on the map
        //network layer invokes put and passing in kv
        //will either insert new key with value, or will update if it already exists
        self.map.insert(key.to_string(), value);
    }
    pub fn delete(&mut self, key: &str){
        //first appending to the WAL
        //so first writing the opcode
        self.file.write_all(&[1u8]).unwrap();
        //then i want to write the length of key
        let key_bytes = key.as_bytes();
        self.file.write_all(&(key_bytes.len() as u32).to_be_bytes()).unwrap();
        //then the key itself
        self.file.write_all(key_bytes).unwrap();
        self.file.sync_all().unwrap();

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
        let _ = std::fs::remove_file("test_put_and_get.log");  // start clean
        let mut storage = Storage::new("test_put_and_get.log");
        storage.put("hello", b"world".to_vec());
        assert_eq!(storage.get("hello"), Some(b"world".to_vec()));
    }

    #[test]
    fn test_delete() {
        let _ = std::fs::remove_file("test_delete.log");  // start clean
        let mut storage = Storage::new("test_delete.log");
        storage.put("hello", b"world".to_vec());
        storage.delete("hello");
        assert_eq!(storage.get("hello"), None);
    }

    #[test]
    fn test_get_missing_key() {
        let _ = std::fs::remove_file("test_get_missing_key.log");  // start clean
        let storage = Storage::new("test_get_missing_key.log");
        assert_eq!(storage.get("missing"), None);
    }

    #[test]
    fn test_replay() {
        let _ = std::fs::remove_file("test_replay.log");  // start clean
        let mut storage = Storage::new("test_replay.log");
        storage.put("hello", b"world".to_vec());
        storage.put("hi", b"john".to_vec());
        storage.put("bye", b"sam".to_vec());
        storage.delete("hi");
        drop(storage);
        let mut storage = Storage::new("test_replay.log");
        assert_eq!(storage.get("hello"), Some(b"world".to_vec()));
        assert_eq!(storage.get("bye"), Some(b"sam".to_vec()));
        assert_eq!(storage.get("hi"), None);
    }
}

fn read_entry(reader: &mut BufReader<File>) -> std::io::Result<Entry> {
    //three cases, either we read EOF ? runs and we break
    //we have a partial entry and so we return ?, break out, return only valid entries
    //a invalid opcode such as 7, else captures and returns error
    let mut op_buf = [0u8; 1];
    reader.read_exact(&mut op_buf)?;
    if op_buf[0] == 0{
        //extract key and value, not sure yet
        let mut key_len_buf = [0u8; 4];
        reader.read_exact(&mut key_len_buf)?;
        let key_len = u32::from_be_bytes(key_len_buf) as usize;
        let mut key_buf = vec![0u8; key_len];
        reader.read_exact(&mut key_buf)?;
        let key = String::from_utf8(key_buf).unwrap();

        //for the value
        let mut val_len_buf = [0u8; 4];
        reader.read_exact(&mut val_len_buf)?;
        let val_len = u32::from_be_bytes(val_len_buf) as usize;
        let mut val_buf = vec![0u8; val_len];
        reader.read_exact(&mut val_buf)?;
        let value = val_buf;
        Ok(Entry::Put { key, value })
    }
    else if op_buf[0] == 1{
        //extract key and value, not sure yet
        let mut key_len_buf = [0u8; 4];
        reader.read_exact(&mut key_len_buf)?;
        let key_len = u32::from_be_bytes(key_len_buf) as usize;
        let mut key_buf = vec![0u8; key_len];
        reader.read_exact(&mut key_buf)?;
        let key = String::from_utf8(key_buf).unwrap();
        Ok(Entry::Delete { key })
    }
    else {
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "unknown opcode",
    ))
}

}