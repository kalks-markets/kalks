import java.util.Properties

plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

// The release key lives OUTSIDE the repo (the founder's Mac: ~/.kalks-keys/kalks-release.jks + key.properties, see
// README "Release signing"). android/key.properties (git-ignored) either holds the standard Flutter fields
// (storeFile, storePassword, keyAlias, keyPassword) or only points at the file that does:
//   keyProperties=~/.kalks-keys/key.properties
// Without a usable key a release build is debug-signed, which is fine for USB testing but never for the website or
// the store (an APK signed with another key can't update the installed app).
fun homePath(p: String): String = if (p.startsWith("~/")) System.getProperty("user.home") + p.substring(1) else p

val keyProps = Properties().apply {
    val f = rootProject.file("key.properties")
    if (f.exists()) f.inputStream().use { load(it) }
    getProperty("keyProperties")?.let { path ->
        val ext = File(homePath(path.trim()))
        if (ext.exists()) ext.inputStream().use { load(it) } else logger.warn("Kalks: $ext not found, release builds are debug-signed")
    }
}
val hasReleaseKey =
    listOf("storeFile", "storePassword", "keyAlias", "keyPassword").all { !keyProps.getProperty(it).isNullOrBlank() } &&
        File(homePath(keyProps.getProperty("storeFile").trim())).exists()

android {
    namespace = "com.kalkstrade.app"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        // One app, "Kalks" (Client Area + Kalks Trader). Android 7.0+ (API 24).
        applicationId = "com.kalkstrade.app"
        minSdk = 24
        targetSdk = flutter.targetSdkVersion
        // Uses the version code from pubspec.yaml. When using split APKs, 1000 * ABI_VERSION
        // is added automatically by Flutter. (https://developer.android.com/studio/build/configure-apk-splits#configure-APK-versions)
        // You can force using the value of versionCode by specifying the `-P force-version-code-ignoring-abi=true`
        // flag during build.
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    signingConfigs {
        if (hasReleaseKey) {
            create("release") {
                storeFile = File(homePath(keyProps.getProperty("storeFile").trim()))
                storePassword = keyProps.getProperty("storePassword")
                keyAlias = keyProps.getProperty("keyAlias")
                keyPassword = keyProps.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            signingConfig = if (hasReleaseKey) signingConfigs.getByName("release") else signingConfigs.getByName("debug")
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}

dependencies {
    // Theme.AppCompat for the launch theme: local_auth's BiometricPrompt crashes on Android 8 and below without it
    implementation("androidx.appcompat:appcompat:1.7.1")
}
