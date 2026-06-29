import sys

with open("crates/rustodian-storage/src/store.rs", "r") as f:
    content = f.read()

# Separate tests mod
if "impl SqliteStore {" in content and "mod tests {" in content:
    # Find the impl block we added at the end
    impl_start = content.find("impl SqliteStore {\n    pub fn get_setting")
    if impl_start != -1:
        impl_block = content[impl_start:]
        rest_of_file = content[:impl_start]

        # Now find the LAST mod tests
        tests_start = rest_of_file.rfind("#[cfg(test)]\nmod tests {")
        if tests_start != -1:
            before_tests = rest_of_file[:tests_start]
            tests_block = rest_of_file[tests_start:]

            with open("crates/rustodian-storage/src/store.rs", "w") as f:
                f.write(before_tests + impl_block + "\n" + tests_block)
