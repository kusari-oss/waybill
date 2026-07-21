// Android-side module with mixed dep configurations to exercise the
// dep-config → lifecycle-scope mapping per US2 AS3.
plugins {
    kotlin("jvm")
}

dependencies {
    implementation(libs.okhttp)
    api("org.jetbrains.kotlin:kotlin-stdlib:1.9.20")
    testImplementation("io.kotest:kotest-runner-junit5:5.8.0")
    kapt("com.google.dagger:dagger-compiler:2.50")
    debugImplementation("com.squareup.leakcanary:leakcanary-android:2.12")
}
