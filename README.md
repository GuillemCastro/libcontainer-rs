# libcontainer-rs - A rust library for creating containers

libcontainer-rs is a rust library for creating containers. Whether you want to build your own container runtime or embed it in your application, this library can help you.

__WARNING__ ⚠️: This library is in an early development stage. Do not use it in production.

## Features

* Embeddable container runtime
* Multiple filesystems for the container root filesystem (overlayfs, tmpfs)

## Non-objectives
I do not plan on working on the following points in the near future, but PRs are welcome.

* Compatibility with Windows, OSX, BSD, or other non-Linux operating systems
* Drop-in replacement for existing container runtimes

## License

This library is licensed under the MIT License

You are free to exercise the freedoms described in the [LICENSE](LICENSE) file. Absolutely no warranties are given of any kind.

```
The MIT License
Copyright (c) 2022 Guillem Castro

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:
The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.
THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```