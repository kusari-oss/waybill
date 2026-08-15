// m235 US3 fixture — subproject A (app) with runtime + test deps.
plugins {
    id("java")
}

dependencies {
    implementation("com.example.waybillfixture:app-runtime-dep:1.0.0")
    testImplementation("com.example.waybillfixture:app-test-dep:2.0.0")
}
