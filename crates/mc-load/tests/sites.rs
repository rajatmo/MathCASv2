//! `Loader::read_recorded`: definition sites and name resolutions, in the input and in imported module files.
use mc_api::{RefStore, Store};
use mc_load::{DefinitionKind, InMemory, Loader, Origin};
use std::path::Path;

fn files() -> InMemory {
    InMemory::new().with("dir/Geo.mc", "# geometry\nmodule Geo {\n  pub fn area(r) = pi*r^2\n  fn helper(x) = x\n}\n")
}

#[test]
fn definitions_and_resolutions_across_an_import() {
    let store = RefStore::new();
    let mut loader = Loader::new(Box::new(files()));
    let src = "import Geo\nfn twice(x) = 2*area(x)\ny = Geo.area(1) + twice(2)\n";
    let origin = Origin::file(Path::new("dir/main.mc"));
    let pi = store.sym("pi");
    let (read, sites) = loader.read_recorded(&store, src, &origin, &|s| s == pi);
    assert!(read.is_ok());

    // the module and its members, in the imported file, at the name as written
    let geo_text = "# geometry\nmodule Geo {\n  pub fn area(r) = pi*r^2\n  fn helper(x) = x\n}\n";
    let module: Vec<_> = sites.definitions_of("Geo", DefinitionKind::Module).collect();
    assert_eq!(module.len(), 1);
    assert_eq!(module[0].origin, Origin::file(&Path::new("dir").join("Geo.mc")));
    assert_eq!(&geo_text[module[0].name_span.start as usize..module[0].name_span.end as usize], "Geo");
    let area: Vec<_> = sites.definitions_of("Geo.area", DefinitionKind::Value).collect();
    assert_eq!(area.len(), 1);
    let at = area[0].name_span.start as usize;
    assert_eq!(&geo_text[at..at + 4], "area");
    assert_eq!(sites.definitions_of("Geo.helper", DefinitionKind::Value).count(), 1);

    // the input's own definitions, bare
    let twice: Vec<_> = sites.definitions_of("twice", DefinitionKind::Value).collect();
    assert_eq!(twice.len(), 1);
    assert_eq!(twice[0].origin, origin);
    assert_eq!(&src[twice[0].name_span.start as usize..twice[0].name_span.end as usize], "twice");
    assert_eq!(sites.definitions_of("y", DefinitionKind::Value).count(), 1);

    // what the names of each statement resolve to; locals (`x`) are not listed
    let in_twice = sites.resolution_at(&origin.name, src.find("area(x)").unwrap() as u32).unwrap();
    assert_eq!(in_twice.get("area"), Some("Geo.area"));
    assert_eq!(in_twice.get("twice"), Some("twice"));
    assert_eq!(in_twice.get("x"), None);
    let in_y = sites.resolution_at(&origin.name, src.find("Geo.area(1)").unwrap() as u32).unwrap();
    assert_eq!(in_y.get("Geo.area"), Some("Geo.area"));
    assert_eq!(in_y.get("twice"), Some("twice"));
    let in_module = sites.resolution_at(&module[0].origin.name, at as u32).unwrap();
    assert_eq!(in_module.get("area"), Some("Geo.area"));
    assert_eq!(in_module.get("pi"), Some("pi"));
}

#[test]
fn sites_survive_an_error_and_plain_reads_are_unchanged() {
    let store = RefStore::new();
    let mut loader = Loader::new(Box::new(files()));
    let src = "fn ok(x) = x\nimport Geo\nGeo.helper(1)\n";
    let origin = Origin::file(Path::new("dir/main.mc"));
    let (read, sites) = loader.read_recorded(&store, src, &origin, &|_| false);
    let err = read.unwrap_err();
    assert!(err.first_message().contains("private"), "{}", err.render());
    assert_eq!(sites.definitions_of("ok", DefinitionKind::Value).count(), 1);
    assert_eq!(sites.definitions_of("Geo", DefinitionKind::Module).count(), 1);
    // the reader state was restored: Geo is not loaded, as after the same error in a plain read
    assert!(loader.module("Geo").is_none());
    assert!(loader.read(&store, src, &origin, &|_| false).is_err());
}
