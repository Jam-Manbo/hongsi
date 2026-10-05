package dev.kyuyoung.hongsi

import android.content.Context
import android.database.Cursor
import android.database.MatrixCursor
import android.os.CancellationSignal
import android.os.Environment
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.provider.DocumentsContract.Document
import android.provider.DocumentsContract.Root
import android.provider.DocumentsProvider
import android.webkit.MimeTypeMap
import java.io.File
import java.io.FileNotFoundException

internal object DownloadFiles {
    const val ROOT_ID = "downloads"
    fun directory(context: Context): File {
        val base = context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)
            ?: throw FileNotFoundException("다운로드 폴더를 찾지 못했어요.")
        return File(base, "홍시").apply { mkdirs() }.canonicalFile
    }
    fun checked(context: Context, path: String): File {
        val file = File(path).canonicalFile
        if (file.parentFile != directory(context) || !file.isFile) {
            throw FileNotFoundException("파일이 없어요.")
        }
        return file
    }
    fun mime(file: File): String = MimeTypeMap.getSingleton()
        .getMimeTypeFromExtension(file.extension.lowercase()) ?: "application/octet-stream"
    fun authority(context: Context) = "${context.packageName}.downloads"
    fun documentUri(context: Context) = DocumentsContract.buildDocumentUri(authority(context), ROOT_ID)
}

class DownloadDocuments : DocumentsProvider() {
    private val columns = arrayOf(Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME,
        Document.COLUMN_MIME_TYPE, Document.COLUMN_FLAGS, Document.COLUMN_SIZE, Document.COLUMN_LAST_MODIFIED)
    override fun onCreate() = true

    override fun queryRoots(projection: Array<out String>?): Cursor {
        val values = mapOf<String, Any?>(Root.COLUMN_ROOT_ID to DownloadFiles.ROOT_ID,
            Root.COLUMN_DOCUMENT_ID to DownloadFiles.ROOT_ID, Root.COLUMN_TITLE to "홍시 받은 파일",
            Root.COLUMN_SUMMARY to "클래스룸 첨부파일", Root.COLUMN_ICON to R.mipmap.ic_launcher,
            Root.COLUMN_FLAGS to Root.FLAG_SUPPORTS_IS_CHILD, Root.COLUMN_MIME_TYPES to "*/*")
        return MatrixCursor(projection ?: values.keys.toTypedArray()).apply {
            addRow(columnNames.map { values[it] })
        }
    }

    private fun file(documentId: String): File {
        if (documentId == DownloadFiles.ROOT_ID) return DownloadFiles.directory(requireNotNull(context))
        if (!documentId.startsWith("file:")) throw FileNotFoundException()
        return DownloadFiles.checked(requireNotNull(context), File(DownloadFiles.directory(requireNotNull(context)), documentId.removePrefix("file:")).path)
    }

    private fun add(cursor: MatrixCursor, documentId: String) {
        val file = file(documentId)
        val root = documentId == DownloadFiles.ROOT_ID
        val values = mapOf<String, Any?>(Document.COLUMN_DOCUMENT_ID to documentId,
            Document.COLUMN_DISPLAY_NAME to if (root) "홍시 받은 파일" else file.name,
            Document.COLUMN_MIME_TYPE to if (root) Document.MIME_TYPE_DIR else DownloadFiles.mime(file),
            Document.COLUMN_FLAGS to 0, Document.COLUMN_SIZE to if (root) null else file.length(),
            Document.COLUMN_LAST_MODIFIED to file.lastModified())
        cursor.addRow(cursor.columnNames.map { values[it] })
    }

    override fun queryDocument(documentId: String, projection: Array<out String>?): Cursor =
        MatrixCursor(projection ?: columns).apply { add(this, documentId) }

    override fun queryChildDocuments(parentDocumentId: String, projection: Array<out String>?, sortOrder: String?): Cursor {
        if (parentDocumentId != DownloadFiles.ROOT_ID) throw FileNotFoundException()
        return MatrixCursor(projection ?: columns).apply {
            DownloadFiles.directory(requireNotNull(context)).listFiles()?.filter { it.isFile }?.sortedBy { it.name }?.forEach {
                try { add(this, "file:${it.name}") } catch (_: FileNotFoundException) { }
            }
        }
    }

    override fun isChildDocument(parentDocumentId: String, documentId: String): Boolean =
        parentDocumentId == DownloadFiles.ROOT_ID && documentId != parentDocumentId &&
            runCatching { file(documentId).isFile }.getOrDefault(false)

    override fun openDocument(documentId: String, mode: String, signal: CancellationSignal?): ParcelFileDescriptor {
        if (mode != "r" || documentId == DownloadFiles.ROOT_ID) throw FileNotFoundException("읽기만 지원해요.")
        return ParcelFileDescriptor.open(file(documentId), ParcelFileDescriptor.MODE_READ_ONLY)
    }
}
