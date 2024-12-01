use std::{env, fs, path::Path};

use typify::{TypeSpace, TypeSpacePatch, TypeSpaceSettings};

fn build_schema(name: &str, root_to_rename: &str) {
    let content = std::fs::read_to_string(format!("schemas/{name}.json")).unwrap();
    let schema = serde_json::from_str::<schemars::schema::RootSchema>(&content).unwrap();
    let rename = Some("ConfigRoot".to_string());
    let mut type_space = TypeSpace::new(
        TypeSpaceSettings::default()
            .with_struct_builder(true)
            .with_derive("Clone".into())
            .with_patch(
                root_to_rename,
                &TypeSpacePatch {
                    rename,
                    derives: vec!["confique::Config".into()],
                },
            ),
    );
    type_space.add_root_schema(schema).unwrap();

    let contents =
        prettyplease::unparse(&syn::parse2::<syn::File>(type_space.to_stream()).unwrap());

    let mut out_file = Path::new(&env::var("OUT_DIR").unwrap()).to_path_buf();
    out_file.push(format!("{name}.rs"));
    fs::write(out_file, contents).unwrap();
}
fn main() {
    build_schema(
        "pyproject",
        "JsonSchemaForPythonProjectMetadataAndConfiguration",
    );
    build_schema("deno", "DenoConfigurationFileSchema");
    // FIXME confique not available on JsonSchemaForNpmPackageJsonFilesVariant because enum
    build_schema("package", "foo")
}
