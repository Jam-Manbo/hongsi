import org.gradle.api.DefaultTask
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.tasks.InputDirectory
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.TaskAction

abstract class GenerateWidgetThemes : DefaultTask() {
    @get:InputDirectory abstract val sourceDirectory: DirectoryProperty
    @get:OutputDirectory abstract val resourceDirectory: DirectoryProperty
    @get:OutputDirectory abstract val javaDirectory: DirectoryProperty

    @TaskAction fun generate() {
        val source = sourceDirectory.get().asFile
        val output = resourceDirectory.get().asFile
        val java = javaDirectory.get().asFile
        output.deleteRecursively()
        java.deleteRecursively()
        val files = listOf("layout", "drawable", "xml").flatMap { type ->
            source.resolve(type).listFiles().orEmpty().filter {
                it.extension == "xml" && (it.name.startsWith("widget_") || it.name.startsWith("ic_widget_") || it.name.contains("_widget_"))
            }.map { type to it }
        }
        val entry = Regex("""<(color|string) name="([^"]+)"[^>]*>[\s\S]*?</\1>""")
        val light = entry.findAll(source.resolve("values/widget.xml").readText()).associate { it.groupValues[2] to it.value }
        val dark = entry.findAll(source.resolve("values-night/widget.xml").readText()).associate { it.groupValues[2] to it.value }
        val palette = light.filterKeys { it.startsWith("widget_") || it in dark }
        val images = source.resolve("drawable-nodpi").listFiles().orEmpty().filter { it.name.endsWith("_widget_preview.png") }
        val references = files.map { (type, file) -> type to file.nameWithoutExtension } +
            images.map { "drawable" to it.nameWithoutExtension } +
            palette.map { (name, xml) -> (if (xml.startsWith("<color")) "color" else "string") to name }
        val lookup = references.toSet()
        val reference = Regex("""@(layout|drawable|color|string|xml)/([a-z0-9_]+)""")
        fun themed(text: String, theme: String) = reference.replace(text) {
            if ((it.groupValues[1] to it.groupValues[2]) in lookup) "${it.value}_$theme" else it.value
        }
        for (theme in listOf("light", "dark")) {
            val values = palette.map { (name, xml) ->
                themed((if (theme == "dark") dark[name] ?: xml else xml).replace("name=\"$name\"", "name=\"${name}_$theme\""), theme)
            }
            output.resolve("values/widget_$theme.xml").apply { parentFile.mkdirs(); writeText("<resources>\n${values.joinToString("\n")}\n</resources>\n") }
            files.forEach { (type, file) ->
                output.resolve("$type/${file.nameWithoutExtension}_$theme.xml").apply { parentFile.mkdirs(); writeText(themed(file.readText(), theme)) }
            }
            images.forEach { file ->
                val night = source.resolve("drawable-night-nodpi/${file.name}")
                val selected = if (theme == "dark" && night.exists()) night else file
                selected.copyTo(output.resolve("drawable-nodpi/${file.nameWithoutExtension}_$theme.png").apply { parentFile.mkdirs() })
            }
        }
        val mappings = references.distinct().joinToString("\n") { (type, name) ->
            "        if (resource == R.$type.$name) return dark ? R.${type}.${name}_dark : R.${type}.${name}_light;"
        }
        java.resolve("dev/kyuyoung/hongsi/widget/WidgetThemeResources.java").apply {
            parentFile.mkdirs()
            writeText("""package dev.kyuyoung.hongsi.widget;
import dev.kyuyoung.hongsi.R;
final class WidgetThemeResources {
    static int themed(int resource, boolean dark) {
$mappings
        return resource;
    }
}
""")
        }
    }
}
