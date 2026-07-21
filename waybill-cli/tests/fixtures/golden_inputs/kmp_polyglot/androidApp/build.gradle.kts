// Android-side module — pure Android dependencies via Maven coords.
plugins {
    kotlin("jvm")
}

dependencies {
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.compose.ui:ui:1.6.0")
    testImplementation("io.kotest:kotest-runner-junit5:5.8.0")
}
