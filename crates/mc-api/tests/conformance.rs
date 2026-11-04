use mc_api::{conformance, RefStore};

#[test]
fn reference_store_conforms() {
    conformance::check_packing(&RefStore::new());
}
