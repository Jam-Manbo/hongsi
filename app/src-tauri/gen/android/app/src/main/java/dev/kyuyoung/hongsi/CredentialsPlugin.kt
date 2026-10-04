package dev.kyuyoung.hongsi

import android.app.Activity
import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONObject
import java.io.File
import java.security.KeyStore
import java.util.concurrent.Executors
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

internal class CredentialVault(context: Context, name: String = "auto-login") {
    private val alias = "${context.packageName}.credentials.$name"
    private val file = AtomicFile(File(context.noBackupFilesDir, "credentials-$name.bin"))
    private fun keystore() = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    private fun key(create: Boolean): SecretKey? {
        (keystore().getKey(alias, null) as? SecretKey)?.let { return it }
        if (!create) return null
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setKeySize(256).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build())
        }.generateKey()
    }

    @Synchronized fun save(secret: String) {
        require(secret.isNotBlank() && secret.length <= 16384)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key(true))
        cipher.updateAAD(alias.toByteArray(Charsets.UTF_8))
        val plaintext = secret.toByteArray(Charsets.UTF_8)
        val encrypted = try { cipher.doFinal(plaintext) } finally { plaintext.fill(0) }
        require(cipher.iv.size == 12)
        val output = file.startWrite()
        try {
            output.write(byteArrayOf(1))
            output.write(cipher.iv)
            output.write(encrypted)
            file.finishWrite(output)
        } catch (e: Exception) { file.failWrite(output); throw e }
    }

    @Synchronized fun load(): String? {
        val bytes = try { file.readFully() } catch (_: java.io.FileNotFoundException) { return null }
        require(bytes.size in 30..65536 && bytes[0] == 1.toByte())
        val storedKey = key(false) ?: error("Credential key unavailable")
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, storedKey, GCMParameterSpec(128, bytes.copyOfRange(1, 13)))
        cipher.updateAAD(alias.toByteArray(Charsets.UTF_8))
        val plaintext = cipher.doFinal(bytes.copyOfRange(13, bytes.size))
        return try { plaintext.toString(Charsets.UTF_8) } finally { plaintext.fill(0) }
    }

    @Synchronized fun clear() {
        file.delete()
        keystore().deleteEntry(alias)
        check(!file.baseFile.exists())
    }
}

@InvokeArg
class SaveCredentialsArgs { lateinit var secret: String }

@TauriPlugin
class CredentialsPlugin(activity: Activity) : Plugin(activity) {
    private val vault = CredentialVault(activity.applicationContext)
    private val worker = Executors.newSingleThreadExecutor()

    @Command fun load(invoke: Invoke) { worker.execute {
        try { invoke.resolve(JSObject().put("secret", vault.load() ?: JSONObject.NULL)) }
        catch (_: Exception) { invoke.reject("자동 로그인 정보를 읽지 못했어요.") }
    } }
    @Command fun save(invoke: Invoke) { worker.execute {
        try { vault.save(invoke.parseArgs(SaveCredentialsArgs::class.java).secret); invoke.resolve() }
        catch (_: Exception) { invoke.reject("자동 로그인 정보를 저장하지 못했어요.") }
    } }
    @Command fun clear(invoke: Invoke) { worker.execute {
        try { vault.clear(); invoke.resolve() }
        catch (_: Exception) { invoke.reject("자동 로그인 정보를 삭제하지 못했어요.") }
    } }
}
