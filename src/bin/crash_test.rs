use raft_kv :: storage::Storage;

fn write(){
    let mut storage = Storage::new("verify.log");
    for i in 0..10000 {
        //building two strings, takes a string and vec
        let key = format!("key:{}", i);
        let value = format!("value:{}", i);
        storage.put(&key, value.into_bytes());
    }
    loop {}
}


fn verify(){
    let mut count = 0;
    let storage = Storage::new("verify.log");
    let mut missing_seen = false;
    let mut gap = false;
    for i in 0..10000{
        let key = format!("key:{}", i);
        let value = format!("value:{}", i);

        match storage.get(&key) {
            Some(stored) => {
                // key is present
                if missing_seen { gap = true; }        // present after an absent one
                if stored == value.into_bytes() { count += 1; }
            }
            None => {
                missing_seen = true;
            }
        }
    }
    println!("{} of 10000 keys present and correct, gap: {}", count, gap);
}

fn main() {
    let mode = std::env::args().nth(1).expect("usage: crash_test <write|verify>");

    match mode.as_str() {
        "write" => write(),
        "verify" => verify(),
        _ => panic!("unknown mode"),
    }
}