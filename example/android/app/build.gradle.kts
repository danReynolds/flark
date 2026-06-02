plugins {
    id("com.android.application")
    id("kotlin-android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    namespace = "dev.flark.editor.example"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = JavaVersion.VERSION_17.toString()
    }

    defaultConfig {
        applicationId = "dev.flark.editor.example"
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.getByName("debug")
        }
    }
}

flutter {
    source = "../.."
}

tasks.register("verifyFlarkComrakNativeLibs") {
    group = "verification"
    description = "Builds the debug APK and checks for the Flark native parser library."
    dependsOn("assembleDebug")

    doLast {
        val bridgeLibraryName = "libflark_comrak_bridge.so"
        val candidates = listOf(
            layout.buildDirectory.file("outputs/apk/debug/app-debug.apk").get().asFile,
            rootProject.layout.projectDirectory.file(
                "../build/app/outputs/flutter-apk/app-debug.apk",
            ).asFile,
        )
        val apk = candidates.firstOrNull { it.isFile }
            ?: error("Debug APK was not found after assembleDebug.")
        val packagedBridgeFiles = zipTree(apk).matching {
            include("lib/**/$bridgeLibraryName")
        }.files

        check(packagedBridgeFiles.isNotEmpty()) {
            "Flark native parser library was not packaged in ${apk.path}."
        }

        logger.lifecycle(
            "Verified $bridgeLibraryName in ${apk.path}: " +
                packagedBridgeFiles.joinToString { it.name },
        )
    }
}
