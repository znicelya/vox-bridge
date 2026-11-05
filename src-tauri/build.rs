fn main() {
    println!("cargo:rerun-if-changed=proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path().expect("vendored protoc"))
        .include_file("volc_ast.rs")
        .compile_protos(
            &["proto/products/understanding/ast/ast_service.proto"],
            &["proto"],
        )
        .expect("compile official Volcengine AST protobuf definitions");
    tauri_build::build()
}
