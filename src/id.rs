use uuid::Uuid;

const BASE62_CHARS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

pub fn new_id() -> String {
    let uuid = Uuid::now_v7();
    let mut n = uuid.as_u128();
    let mut chars = [b'0'; 22];

    for c in chars.iter_mut().rev() {
        *c = BASE62_CHARS[(n % 62) as usize];
        n /= 62;
    }

    String::from_utf8(chars.to_vec()).expect("base62 chars are valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_id_length() {
        let id = new_id();
        assert_eq!(id.len(), 22);
    }

    #[test]
    fn test_new_id_uniqueness() {
        let id1 = new_id();
        let id2 = new_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_new_id_chars() {
        let id = new_id();
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
