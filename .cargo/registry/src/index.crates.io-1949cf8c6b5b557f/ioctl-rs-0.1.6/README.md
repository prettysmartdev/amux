# IOCTL

The `ioctl-rs` crate provides raw definitions as well as safe bindings for system ioctl calls on
Unix operating systems. The ioctl constants, e.g., `TIOCEXCL`, are exported as constants that are
usable with the unsafe `ioctl()` function. Wrapper functions are provided for some ioctls that
handle input and output parameters and convert return values to `io::Result`. The wrapper functions
are named after the ioctl that they implement. For example, `TIOCEXCL` is implemented in a wrapper
function named `tiocexcl()`.

## Usage
Add `ioctl-rs` as a dependency in `Cargo.toml`:

```toml
[dependencies]
ioctl-rs = "0.1"
```

Import the `ioctl_rs` crate. You may also need `std::os::unix::io::RawFd` for file descriptors and
`std::io::Result` to propagate errors.

```rust
extern crate ioctl_rs as ioctl;

use std::io
use std::os::unix::io::RawFd;

fn setup_serial_port(fd: RawFd) -> io::Result<()> {
    // put file descriptor in exclusive mode
    try!(ioctl::tiocexcl(fd));

    // clear all the modem pins
    ioctl::tiocmset(fd, 0)
}
```

## Contributing

This repository includes a C++ program called `ioctl_list`, which generates Rust definitions for
ioctl constants. To contribute ioctl definitions for a new platform or to add new ioctl
definitions, please follow the instructions in the `ioctl_list` [README](/ioctl_list/README.md).

### Contributors

* [dcuddeback](https://github.com/dcuddeback)
* [trlim](https://github.com/trlim)
* [mneumann](https://github.com/mneumann)

## License
Copyright © 2015 David Cuddeback

Distributed under the [MIT License](LICENSE).
