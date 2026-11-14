//! Round 41: further message digests (MD2, MD4, RIPEMD-160, SHA-3, Keccak, BLAKE2 of any length, BLAKE2X), byte
//! arrays hashed by their bytes, and more output formats. Values are the published test vectors of each
//! specification; compositions are checked against their parts.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn extendable_output_and_lengths() {
    let mut e = CoreEngine::new();
    let x = ev(&mut e, "hash(\"x\", [\"BLAKE2xs\", 2400], \"HexString\")");
    assert!(x.starts_with("\"65105adc09946a6533e0b3c66a7ec1ac802b24f109d95356c9b5ba9b16f4a2846b13aec10774fbb61549cafd0c2994"), "{x}");
    assert_eq!(x.len(), 600 + 2);
    // the length is a parameter of BLAKE2: RFC 7693's vector for 512 bits, and a shorter digest is another hash
    assert_eq!(
        ev(&mut e, "hash(\"abc\", [\"BLAKE2b\", 512], \"HexString\")"),
        "\"ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923\""
    );
    assert_eq!(ev(&mut e, "hash(\"abc\", [\"BLAKE2b\", 8], \"HexString\")"), "\"6b\"");
    assert_eq!(ev(&mut e, "hash(\"abc\", \"BLAKE2s\", \"HexString\")"), "\"508c5e8c327c14e2e1a72ba34eeb452f37458b209ed63a294d999b4c86675982\"");
    // a prefix of the extendable output for a shorter length is not the shorter output (the length is hashed)
    assert_eq!(ev(&mut e, "string_take(hash(\"x\", [\"BLAKE2xs\", 256], \"HexString\"), 8) == string_take(hash(\"x\", [\"BLAKE2xs\", 2400], \"HexString\"), 8)"), "false");
}

#[test]
fn named_digests() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "hash(\"abc\", \"MD2\", \"HexString\")"), "\"da853b0d3f88d99b30283a69e6ded6bb\"");
    assert_eq!(ev(&mut e, "hash(\"abc\", \"MD4\", \"HexString\")"), "\"a448017aaf21d8525fc10ae87aa6729d\"");
    assert_eq!(ev(&mut e, "hash(\"abc\", \"RIPEMD160\", \"HexString\")"), "\"8eb208f7e05d987a9b044a8e98c6b087f15a0bfc\"");
    assert_eq!(ev(&mut e, "hash(\"abc\", \"SHA3-256\", \"HexString\")"), "\"3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532\"");
    assert_eq!(ev(&mut e, "hash(\"\", \"Keccak256\", \"HexString\")"), "\"c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470\"");
    // compositions: the digest of the digest's bytes
    assert_eq!(ev(&mut e, "hash(hash(1, \"SHA256\", \"ByteArray\"), \"RIPEMD160\") == hash(1, \"RIPEMD160SHA256\")"), "true");
    assert_eq!(ev(&mut e, "hash(hash(\"a\", \"SHA256\", \"ByteArray\"), \"SHA256\") == hash(\"a\", \"SHA256SHA256\")"), "true");
    assert_eq!(ev(&mut e, "8*length(hash(sin(x), \"SHA256SHA256\", \"ByteArray\"))"), "256");
    assert_eq!(ev(&mut e, "normal(hash(\"a\", \"MD5\", \"ByteArray\")) == hash(\"a\", \"MD5\", \"ByteList\")"), "true");
}

#[test]
fn byte_arrays_and_formats() {
    let mut e = CoreEngine::new();
    // a byte array is hashed by its bytes: the same as the string of those bytes
    assert_eq!(ev(&mut e, "hash(byte_array(to_character_code(\"abc\")), \"MD5\")"), "191415658344158766168031473277922803570");
    assert_eq!(ev(&mut e, "hash(byte_array([225, 136, 180]), \"MD5\") == hash(\"\\u{1234}\", \"MD5\")"), "true");
    assert_eq!(ev(&mut e, "hash(byte_array([123, 45, 67, 89, 98, 76, 54, 32, 1]), \"SHA\")"), "205163851794085253373860936160163003382114085276");
    // the formats of one digest agree: base 36 and base 64 against the integer
    assert_eq!(ev(&mut e, "hash(\"363\", \"MD5\", \"Base36String\")"), "\"ji6oi82cyusjyl2lokcblvp\"");
    assert_eq!(ev(&mut e, "from_digits(map(c => if c >= 97 then c - 87 else c - 48, to_character_code(hash(\"363\", \"MD5\", \"Base36String\"))), 36) == hash(\"363\", \"MD5\")"), "true");
    assert_eq!(ev(&mut e, "hash(\"363\", \"MD5\", \"Base64Encoding\")"), "\"AEEUYPfJLSEkpn6g9MtfhQ==\"");
}
