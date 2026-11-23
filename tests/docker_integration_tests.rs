mod common;

fn run_bash_version_test(docker_bash_version: &str) {
    println!("Testing Bash version: {}", docker_bash_version);

    let target = format!(
        "bash-integration-test-{}",
        docker_bash_version.replace('.', "_")
    );

    common::run_bake_target(&target)
        .unwrap_or_else(|_| panic!("Test failed for Bash version: {}", docker_bash_version));

    println!(
        "Successfully tested Bash {} with flyline",
        docker_bash_version
    );
}

macro_rules! bash_integration_test {
    ($name:ident, $version:expr) => {
        #[test]
        fn $name() {
            run_bash_version_test($version)
        }
    };
}

bash_integration_test!(test_bash_5_1, "5.1");
bash_integration_test!(test_bash_5_2, "5.2");
bash_integration_test!(test_bash_5_3, "5.3");
