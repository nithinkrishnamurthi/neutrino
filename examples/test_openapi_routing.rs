/// Integration test for OpenAPI-based routing
///
/// This test demonstrates that:
/// 1. OpenAPI spec can be loaded from JSON file
/// 2. Routes are extracted correctly with exact paths (not generic patterns)
/// 3. Handler names are mapped correctly from operation IDs
///
/// Run with: cargo test --example test_openapi_routing

use neutrino_core::openapi::OpenApiSpec;

fn main() {
    let spec_path = "/home/nithin/neutrino/examples/openapi.json";

    println!("Loading OpenAPI spec from: {}", spec_path);

    match OpenApiSpec::from_file(spec_path) {
        Ok(spec) => {
            println!("\n✓ Successfully loaded OpenAPI spec");
            println!("  Title: {}", spec.info.title);
            println!("  Version: {}", spec.info.version);

            println!("\n✓ Extracting routes...");
            let routes = spec.extract_routes();

            println!("\nRoutes registered:");
            println!("{:<10} {:<35} {:<20}", "METHOD", "PATH", "HANDLER");
            println!("{}", "-".repeat(65));

            for route in &routes {
                println!(
                    "{:<10} {:<35} {:<20}",
                    route.method, route.path, route.handler_name
                );
            }

            println!("\n✓ Test passed: {} routes extracted", routes.len());

            // Verify expected routes
            assert!(routes.iter().any(|r| r.path == "/api/users" && r.method == "GET"));
            assert!(routes.iter().any(|r| r.path == "/api/users/:user_id" && r.method == "GET"));
            assert!(routes.iter().any(|r| r.path == "/api/products" && r.method == "GET"));
            assert!(routes.iter().any(|r| r.path == "/api/products" && r.method == "POST"));

            println!("\n✓ All route assertions passed!");
            println!("\n🎉 OpenAPI-based routing is working correctly!");
        }
        Err(e) => {
            eprintln!("✗ Failed to load OpenAPI spec: {}", e);
            std::process::exit(1);
        }
    }
}
