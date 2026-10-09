import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.ksp)
    alias(libs.plugins.room)
}

// The release key and its passwords live in the repository's local/ folder,
// which git ignores. A checkout without them still builds a release, unsigned.
val repoRoot: File = rootProject.projectDir.parentFile
val keystore =
    Properties().apply {
        val file = repoRoot.resolve("local/keystore.properties")
        if (file.exists()) file.inputStream().use(::load)
    }

android {
    namespace = "io.github.vivekg7.dhun"
    compileSdk = 37

    defaultConfig {
        applicationId = "io.github.vivekg7.dhun"
        // Android 12 and newer only (docs/plans/011_android_app.md).
        minSdk = 31
        targetSdk = 37
        versionCode = 9
        versionName = "0.9.0"
    }

    signingConfigs {
        if (keystore.isNotEmpty()) {
            create("release") {
                storeFile = repoRoot.resolve(keystore.getProperty("storeFile"))
                storePassword = keystore.getProperty("storePassword")
                keyAlias = keystore.getProperty("keyAlias")
                keyPassword = keystore.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
        }
        release {
            signingConfig = signingConfigs.findByName("release")
            isMinifyEnabled = true
            isShrinkResources = true
            vcsInfo { include = false }
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }

    // English only: the dependencies' translations would otherwise ship too.
    androidResources { localeFilters += listOf("en") }

    buildFeatures {
        compose = true
        buildConfig = true
        resValues = false
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // Play's signed dependency list: nothing here is published through Play.
    dependenciesInfo {
        includeInApk = false
        includeInBundle = false
    }

    packaging {
        resources.excludes += setOf("META-INF/*.kotlin_module", "kotlin/**", "DebugProbesKt.bin", "META-INF/*.version")
    }
}

room3 {
    // Exported schemas are what a migration is written and tested against.
    schemaDirectory("$projectDir/schemas")
}

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.process)
    implementation(libs.media3.exoplayer)
    implementation(libs.media3.session)
    implementation(libs.media3.okhttp)
    implementation(libs.room.runtime)
    implementation(libs.sqlite.framework)
    ksp(libs.room.compiler)
    implementation(libs.okhttp)
    implementation(libs.serialization.json)
    implementation(libs.coroutines.android)
    testImplementation(libs.junit)
}
