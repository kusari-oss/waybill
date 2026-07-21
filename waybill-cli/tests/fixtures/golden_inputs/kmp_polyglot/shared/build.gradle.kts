// KMP shared module — declares deps in source-set blocks so the
// `waybill:kmp-source-set` provenance annotation surfaces.
plugins {
    kotlin("multiplatform")
}

kotlin {
    jvm()
    iosX64()
    sourceSets {
        commonMain {
            dependencies {
                implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.2")
                implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.7.3")
            }
        }
        androidMain {
            dependencies {
                implementation("androidx.lifecycle:lifecycle-viewmodel:2.7.0")
            }
        }
    }
}
