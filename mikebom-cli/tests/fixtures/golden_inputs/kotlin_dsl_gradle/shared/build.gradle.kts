// KMP shared module — exercises the mikebom:kmp-source-set annotation.
// `kotlinx-serialization-json` is declared in BOTH commonMain AND jvmMain
// so the merged source-set array contains both names.
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
            }
        }
        jvmMain {
            dependencies {
                implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.2")
                implementation(libs.ktor.client.cio)
            }
        }
    }
}
