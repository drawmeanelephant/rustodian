import re

with open("crates/rustodian-remote/src/downloader.rs", "r") as f:
    content = f.read()

# Let's see how the mock tarball is constructed in the test:
# "root/foo/bar" inside a symlink named 'foo' pointing to system_dir
# Wait, the error is: Err(Internal("Cannot create a file when that file already exists. (os error 183)"))
# On Windows, creating a symlink in a tar extraction might behave differently or `std::fs::create_dir_all`
# might fail if `parent` is a symlink or conflicts.
# The error happens during the unpack phase probably? Let's check.
