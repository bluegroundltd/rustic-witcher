# Source Code Tests

There are several added tests that ensure the proper functionality of the built transformations available. It is suggested to enhance tests when adding new transformation configurations.

## Prerequisites

Before running tests, ensure you have the following installed:
- [Rust](https://www.rust-lang.org/)
- [Cargo](https://doc.rust-lang.org/cargo/)

## Execution

In order to check if tests are successful, you can run the following:

```shell
brew install cargo-nextest
make run_tests_open_source
```

Example output:
```shell
╰─$ make run_tests_open_source
cargo nextest run --all --features rustic-anonymization-operator/open_source
   Compiling rustic-anonymization-operator v0.1.0 (/Users/nikolaos.nikitas/blueground/rustic-witcher/rustic-anonymization-operator)
   Compiling rustic-cdc-operator v0.1.0 (/Users/nikolaos.nikitas/blueground/rustic-witcher/rustic-cdc-operator)
   Compiling rustic-witcher v0.1.0 (/Users/nikolaos.nikitas/blueground/rustic-witcher)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 4.12s
    Starting 14 tests across 18 binaries (run ID: 265dc796-68e3-437b-80e4-1be0e1869ebb, nextest profile: default)
        PASS [   0.028s] rustic-anonymization-config tests::test_deserialize_config
        PASS [   0.028s] rustic-anonymization-config tests::test_deserialize_config_with_specific_fake_operation
        PASS [   0.037s] rustic-bg-whole-table-transformator tests::it_works
        PASS [   0.040s] rustic-base-transformations replace_transformator::tests::test_replace_transformator
        PASS [   0.040s] rustic-faker-transformations faker_transformators::tests::fake_address_transformator::tests::test_fake_address_transformator
        PASS [   0.040s] rustic-faker-transformations faker_transformators::tests::fake_companyname_transformator::tests::test_fake_company_name_transformator
        PASS [   0.039s] rustic-faker-transformations faker_transformators::tests::fake_email_transformator::tests::test_fake_email_transformator
        PASS [   0.038s] rustic-faker-transformations faker_transformators::tests::fake_firstname_transformator::tests::test_fake_firstname_transformator
        PASS [   0.026s] rustic-faker-transformations faker_transformators::tests::fake_md5_transformator::tests::test_fake_md5_transformator
        PASS [   0.028s] rustic-faker-transformations faker_transformators::tests::fake_lastname_transformator::tests::test_fake_lastname_transformator
        PASS [   0.017s] rustic-faker-transformations faker_transformators::tests::fake_multi_email_transformator::tests::test_transform
        PASS [   0.017s] rustic-faker-transformations faker_transformators::tests::fake_name_transformator::tests::test_fake_name_transformator
        PASS [   0.013s] rustic-faker-transformations faker_transformators::tests::fake_phone_transformator::tests::test_fake_phone_transformator
        PASS [   0.015s] rustic-local-data-importer-cli tests::it_works
------------
     Summary [   0.064s] 14 tests run: 14 passed, 0 skipped
```
