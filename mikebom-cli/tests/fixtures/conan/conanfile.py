from conan import ConanFile


class TestProjectConan(ConanFile):
    name = "test-project"
    version = "0.1.0"
    settings = "os", "compiler", "build_type", "arch"
    requires = ["zlib/1.2.13", "openssl/3.0.0"]
    tool_requires = ["cmake/3.27.0"]
