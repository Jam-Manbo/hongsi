import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("com.google.gms.google-services")
    id("rust")
}

val distribution = providers.environmentVariable("HONGSI_ANDROID_DISTRIBUTION").orElse("direct").get()
require(distribution in setOf("direct", "play")) { "HONGSI_ANDROID_DISTRIBUTION must be direct or play" }

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

android {
    compileSdk = 37
    namespace = "dev.kyuyoung.hongsi"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "dev.kyuyoung.hongsi"
        minSdk = 24
        targetSdk = 37
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
        buildConfigField("String", "HONGSI_UPDATE_URL", "\"https://hongsi.kyuyoung.dev/api/app-update\"")
    }
    val signingProperties = Properties().apply {
        val config = rootProject.file("keystore.properties")
        if (config.exists()) config.inputStream().use { load(it) }
    }
    signingConfigs {
        create("release") {
            if (signingProperties.isNotEmpty()) {
                keyAlias = signingProperties.getProperty("keyAlias")
                keyPassword = signingProperties.getProperty("keyPassword", signingProperties.getProperty("password"))
                storeFile = signingProperties.getProperty("storeFile")?.let { rootProject.file(it) }
                storePassword = signingProperties.getProperty("storePassword", signingProperties.getProperty("password"))
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            if (signingProperties.isNotEmpty()) signingConfig = signingConfigs.getByName("release")
            optimization {
               enable = true
            }
            proguardFiles(
                *fileTree(".") {
                  include("**/*.pro")
                  exclude("build/**")
                }.files.toTypedArray()
            )
        }
    }
    sourceSets {
        getByName("main").java.directories.add("src/$distribution/java")
        getByName("debug").manifest.srcFile("src/$distribution/AndroidManifest.xml")
        getByName("release").manifest.srcFile("src/$distribution/AndroidManifest.xml")
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }
    buildFeatures {
        buildConfig = true
    }
}

val widgetThemes = tasks.register<GenerateWidgetThemes>("generateWidgetThemes") {
    sourceDirectory.set(layout.projectDirectory.dir("src/main/res"))
    resourceDirectory.set(layout.buildDirectory.dir("generated/widgetThemes/res"))
    javaDirectory.set(layout.buildDirectory.dir("generated/widgetThemes/java"))
}
androidComponents.onVariants { variant ->
    variant.sources.res?.addGeneratedSourceDirectory(widgetThemes, GenerateWidgetThemes::resourceDirectory)
    variant.sources.java?.addGeneratedSourceDirectory(widgetThemes, GenerateWidgetThemes::javaDirectory)
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_21
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    if (distribution == "play") implementation("com.google.android.play:app-update:2.1.0")
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
}

apply(from = file("tauri.build.gradle.kts"))
