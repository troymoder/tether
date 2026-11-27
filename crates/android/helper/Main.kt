// The helper runs on the device under app_process, as scrcpy's server does,
// so labels, icons and the gallery come from Android's own APIs.
//
// A request is its id (u32), its kind (u8) and its arguments, a count (u32)
// of strings each prefixed with its length (u32). A reply is the request's
// id, a status byte (0 for done, 1 for failed) and then either the reason, or
// a count of records, each a count of fields, each prefixed with its length.
// Every number is big-endian.

@file:JvmName("Main")

package tether

import android.content.AttributionSource
import android.content.ContentResolver
import android.app.Application
import android.content.Context
import android.content.ContextWrapper
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.content.res.AssetManager
import android.content.res.Resources
import android.database.Cursor
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Matrix
import android.media.ExifInterface
import android.media.MediaMetadataRetriever
import android.media.ThumbnailUtils
import android.os.Binder
import android.os.Build
import android.os.Bundle
import android.os.IBinder
import android.os.Looper
import android.os.Process
import android.provider.MediaStore
import android.util.Size
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.EOFException
import java.io.File
import java.io.FileDescriptor
import java.io.FileOutputStream
import java.io.IOException
import java.io.RandomAccessFile
import kotlin.concurrent.thread

const val APPS = 1
const val THUMBNAILS = 2
const val COUNT = 3
const val PAGE = 4
const val READ = 5

const val ICON = 96
const val THUMBNAIL = 256

// The shell's package, which MediaStore checks the caller against, as it
// does for the `content` command.
const val SHELL = "com.android.shell"

typealias Record = List<ByteArray>

val out = DataOutputStream(BufferedOutputStream(FileOutputStream(FileDescriptor.out)))

// prepareMainLooper is deprecated for apps, because Android creates their
// main looper. app_process creates none, and the ActivityThread built here
// needs one, as in scrcpy's server.
@Suppress("DEPRECATION")
fun main() {
    Looper.prepareMainLooper()

    // Built before any request, because Android's own code also reaches for
    // the current application, as the video thumbnail call does, even where
    // this code never uses the context.
    context

    val input = DataInputStream(BufferedInputStream(System.`in`))

    while (true) {
        val id: Int
        val kind: Int
        val arguments: List<String>

        try {
            id = input.readInt()
            kind = input.readUnsignedByte()
            arguments = List(input.readInt()) { String(bytes(input)) }
        } catch (_: EOFException) {
            break
        }

        // Each request gets its own thread, because a thumbnail batch takes
        // seconds and the next gallery page should not wait behind it.
        thread {
            // ActivityThread creates a Handler, which needs a Looper on the
            // thread that first builds the context.
            Looper.prepare()

            val reply = try {
                Result.success(answer(kind, arguments))
            } catch (error: Throwable) {
                Result.failure(error)
            }

            send(id, reply)
        }
    }

    System.exit(0)
}

fun bytes(input: DataInputStream): ByteArray {
    val bytes = ByteArray(input.readInt())

    input.readFully(bytes)

    return bytes
}

fun send(id: Int, reply: Result<List<Record>>) {
    synchronized(out) {
        out.writeInt(id)

        reply.fold(
            { records ->
                out.writeByte(0)
                out.writeInt(records.size)

                for (record in records) {
                    out.writeInt(record.size)

                    for (field in record) {
                        out.writeInt(field.size)
                        out.write(field)
                    }
                }
            },
            { error ->
                val reason = reasonOf(error)

                out.writeByte(1)
                out.writeInt(reason.size)
                out.write(reason)
            },
        )

        out.flush()
    }
}

fun answer(kind: Int, arguments: List<String>): List<Record> = when (kind) {
    APPS -> arguments.map { id -> app(id) }
    THUMBNAILS -> arguments.map { path -> item(path) { Pair("", thumbnail(path)) } }
    COUNT -> listOf(listOf(gallery(arguments).use { it.count }.toString().toByteArray()))
    PAGE -> page(arguments[0].toInt(), arguments[1].toInt(), arguments.drop(2))
    READ -> listOf(listOf(slice(arguments[0], arguments[1].toLong(), arguments[2].toInt())))
    else -> throw IllegalArgumentException("unknown request $kind")
}

fun record(id: String, reason: ByteArray, label: ByteArray, version: ByteArray, image: ByteArray): Record =
    listOf(id.toByteArray(), reason, label, version, image)

fun item(id: String, made: () -> Pair<String, ByteArray>): Record = try {
    val (label, image) = made()

    record(id, ByteArray(0), label.toByteArray(), ByteArray(0), image)
} catch (error: Throwable) {
    record(id, reasonOf(error), ByteArray(0), ByteArray(0), ByteArray(0))
}

// An error and its chain of causes, because Android wraps the real failure,
// as loadDrawable does with "Drawable … with resource ID …".
fun reasonOf(error: Throwable): ByteArray = generateSequence(error) { it.cause }
    .take(8)
    .joinToString(": caused by ") { it.toString() }
    .toByteArray()

// app_process has no Context of its own, so this builds the system context
// the way scrcpy's Workarounds does. Hidden APIs are open to app_process,
// because zygote does not start it.
val context: Context by lazy {
    val type = Class.forName("android.app.ActivityThread")
    val constructor = type.getDeclaredConstructor()

    constructor.isAccessible = true

    val activityThread = constructor.newInstance()
    val current = type.getDeclaredField("sCurrentActivityThread")
    val system = type.getDeclaredField("mSystemThread")

    current.isAccessible = true
    current.set(null, activityThread)
    system.isAccessible = true
    system.setBoolean(activityThread, true)

    val made = type.getDeclaredMethod("getSystemContext").invoke(activityThread) as Context

    // Some drawables, such as Chrome's icon, reach for the running app's
    // resources through ActivityThread.currentApplication(), which is null
    // here. An Application on the system context fills that in, as scrcpy's
    // fillAppContext does.
    val application = Application()
    val attach = ContextWrapper::class.java.getDeclaredMethod("attachBaseContext", Context::class.java)
    val initial = type.getDeclaredField("mInitialApplication")

    attach.isAccessible = true
    attach.invoke(application, made)
    initial.isAccessible = true
    initial.set(activityThread, application)

    made
}

// Matches disabled apps as well, because `pm list packages` lists them too.
const val MATCHED = PackageManager.MATCH_DISABLED_COMPONENTS or PackageManager.MATCH_UNINSTALLED_PACKAGES

// An app with no icon of its own gets no image and no reason, because
// Android's default icon is a framework drawable the system context cannot
// load. The page shows its own placeholder.
fun app(id: String): Record {
    val packages = context.packageManager
    val info = try {
        packages.getApplicationInfo(id, MATCHED)
    } catch (error: Throwable) {
        return record(id, reasonOf(error), ByteArray(0), ByteArray(0), ByteArray(0))
    }
    val label = labelOf(packages, info).toByteArray()
    val version = versionOf(packages, id).toByteArray()

    if (info.icon == 0) {
        return record(id, ByteArray(0), label, version, ByteArray(0))
    }

    return try {
        val icon = resourcesOf(info).getDrawableForDensity(info.icon, ICON_DENSITY, null)
            ?: throw Resources.NotFoundException("no icon")
        val bitmap = Bitmap.createBitmap(ICON, ICON, Bitmap.Config.ARGB_8888)

        icon.setBounds(0, 0, ICON, ICON)
        icon.draw(Canvas(bitmap))

        record(id, ByteArray(0), label, version, encoded(bitmap, Bitmap.CompressFormat.PNG))
    } catch (error: Throwable) {
        record(id, reasonOf(error), label, version, ByteArray(0))
    }
}

@Suppress("DEPRECATION")
fun versionOf(packages: PackageManager, id: String): String = try {
    packages.getPackageInfo(id, MATCHED).versionName ?: ""
} catch (_: PackageManager.NameNotFoundException) {
    ""
}

// PackageManager's getApplicationLabel cannot reach an app's resources
// through the system context built here and returns the package name, so it
// is the last resort.
fun labelOf(packages: PackageManager, info: ApplicationInfo): String {
    info.nonLocalizedLabel?.let { return it.toString() }

    if (info.labelRes != 0) {
        try {
            return resourcesOf(info).getText(info.labelRes).toString()
        } catch (_: Resources.NotFoundException) {
        }
    }

    return packages.getApplicationLabel(info).toString()
}

// The density icons are drawn at, about 96px for a 48dp launcher icon.
const val ICON_DENSITY = 320

// PackageManager's getApplicationIcon cannot load an app's resources through
// the system context built here. The hidden addAssetPath and the deprecated
// Resources constructor are the only way to build an AssetManager outside a
// Context.
@Suppress("DEPRECATION")
fun resourcesOf(info: ApplicationInfo): Resources {
    val assets = AssetManager::class.java.getDeclaredConstructor().newInstance()
    val add = AssetManager::class.java.getMethod("addAssetPath", String::class.java)
    val system = Resources.getSystem()

    add.invoke(assets, info.sourceDir)
    info.splitSourceDirs?.forEach { split -> add.invoke(assets, split) }

    // An app such as Chrome refers to resources in a shared library
    // (TrichromeLibrary). Android adds those as shared libraries after the
    // app, so each gets its own package id. Added as a plain asset path, the
    // library would claim the app's id 0x7f and hide its resources.
    val library = AssetManager::class.java.getMethod("addAssetPathAsSharedLibrary", String::class.java)

    info.sharedLibraryFiles?.forEach { file -> library.invoke(assets, file) }

    return Resources(assets, system.displayMetrics, system.configuration)
}

// Made from the file itself, because MediaStore's loadThumbnail goes through
// the system context's ContentResolver, which cannot reach a provider. The
// activity manager refuses an ActivityThread it did not start. Before Android
// 10 the only video thumbnail call is the one deprecated since.
@Suppress("DEPRECATION")
fun thumbnail(path: String): ByteArray {
    val file = File(path)
    val made = if (Build.VERSION.SDK_INT >= 29) {
        val size = Size(THUMBNAIL, THUMBNAIL)

        try {
            ThumbnailUtils.createImageThumbnail(file, size, null)
        } catch (_: IOException) {
            frame(path)
        }
    } else {
        picture(path)
            ?: ThumbnailUtils.createVideoThumbnail(path, MediaStore.Images.Thumbnails.MINI_KIND)
            ?: throw IOException("no thumbnail")
    }

    return encoded(fit(made), Bitmap.CompressFormat.JPEG)
}

// Read straight from MediaMetadataRetriever, because the createVideoThumbnail
// that wraps it throws a NullPointerException in app_process on some videos.
// Time -1 returns nothing on some builds, so the first key frame and the
// first frame by index are tried next, then any cover art.
fun frame(path: String): Bitmap {
    val retriever = MediaMetadataRetriever()

    try {
        retriever.setDataSource(path)

        val tries = sequence {
            yield { retriever.getScaledFrameAtTime(-1, MediaMetadataRetriever.OPTION_CLOSEST_SYNC, THUMBNAIL, THUMBNAIL) }
            yield { retriever.getFrameAtTime(0, MediaMetadataRetriever.OPTION_CLOSEST_SYNC) }

            if (Build.VERSION.SDK_INT >= 28) {
                yield { retriever.getFrameAtIndex(0) }
            }

            yield { retriever.embeddedPicture?.let { art -> BitmapFactory.decodeByteArray(art, 0, art.size) } }
        }

        for (attempt in tries) {
            val made = try {
                attempt()
            } catch (_: RuntimeException) {
                null
            }

            if (made != null) {
                return made
            }
        }

        val kind = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_MIMETYPE)
        val width = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH)
        val height = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT)

        throw IOException("no frame: $kind ${width}x$height")
    } finally {
        retriever.release()
    }
}

// A photo decoded at reduced size and rotated upright, for Android before 10,
// where ThumbnailUtils does not do this.
fun picture(path: String): Bitmap? {
    val bounds = BitmapFactory.Options()

    bounds.inJustDecodeBounds = true
    BitmapFactory.decodeFile(path, bounds)

    if (bounds.outWidth <= 0) {
        return null
    }

    val options = BitmapFactory.Options()

    options.inSampleSize = maxOf(1, minOf(bounds.outWidth, bounds.outHeight) / THUMBNAIL)

    val bitmap = BitmapFactory.decodeFile(path, options) ?: return null
    val orientation = try {
        ExifInterface(path).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL)
    } catch (_: IOException) {
        ExifInterface.ORIENTATION_NORMAL
    }
    val degrees = when (orientation) {
        ExifInterface.ORIENTATION_ROTATE_90 -> 90f
        ExifInterface.ORIENTATION_ROTATE_180 -> 180f
        ExifInterface.ORIENTATION_ROTATE_270 -> 270f
        else -> return bitmap
    }
    val turned = Matrix()

    turned.postRotate(degrees)

    return Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, turned, true)
}

fun fit(bitmap: Bitmap): Bitmap {
    val scale = THUMBNAIL.toFloat() / maxOf(bitmap.width, bitmap.height)

    if (scale >= 1f) {
        return bitmap
    }

    val width = maxOf(1, (bitmap.width * scale).toInt())
    val height = maxOf(1, (bitmap.height * scale).toInt())

    return Bitmap.createScaledBitmap(bitmap, width, height, true)
}

fun slice(path: String, offset: Long, length: Int): ByteArray = RandomAccessFile(path, "r").use { file ->
    val bytes = ByteArray(minOf(length.toLong(), maxOf(0L, file.length() - offset)).toInt())

    file.seek(offset)
    file.readFully(bytes)

    bytes
}

fun encoded(bitmap: Bitmap, format: Bitmap.CompressFormat): ByteArray {
    val bytes = ByteArrayOutputStream()

    bitmap.compress(format, 80, bytes)

    return bytes.toByteArray()
}

// MediaStore's provider, obtained outside any app as the `content` command
// does, because the system context's ContentResolver is refused.
val media: Any by lazy {
    val manager = if (Build.VERSION.SDK_INT >= 26) {
        Class.forName("android.app.ActivityManager").getMethod("getService").invoke(null)
    } else {
        Class.forName("android.app.ActivityManagerNative").getMethod("getDefault").invoke(null)
    }
    val type = Class.forName("android.app.IActivityManager")
    // User 0, which `content` uses by default.
    val holder = if (Build.VERSION.SDK_INT >= 29) {
        type.getMethod(
            "getContentProviderExternal",
            String::class.java,
            Int::class.javaPrimitiveType,
            IBinder::class.java,
            String::class.java,
        ).invoke(manager, "media", 0, Binder(), "tether")
    } else {
        type.getMethod(
            "getContentProviderExternal",
            String::class.java,
            Int::class.javaPrimitiveType,
            IBinder::class.java,
        ).invoke(manager, "media", 0, Binder())
    } ?: throw IOException("no media provider")

    holder.javaClass.getField("provider").get(holder)!!
}

val COLUMNS = arrayOf("_data", "_size", "date_modified", "media_type", "_id")

fun gallery(folders: List<String>, limit: Int? = null, offset: Int = 0): Cursor {
    val each = "(_data LIKE ? AND _data NOT LIKE ?)"
    val selection = "(media_type=1 OR media_type=3) AND (" + folders.joinToString(" OR ") { each } + ")"
    val arguments = folders.flatMap { listOf("$it/%", "$it/%/%") }.toTypedArray()
    val sort = "date_modified DESC, _id DESC"
    val uri = MediaStore.Files.getContentUri("external")
    val query = media.javaClass.methods.first { it.name == "query" }
    val bundle = Bundle()

    bundle.putString(ContentResolver.QUERY_ARG_SQL_SELECTION, selection)
    bundle.putStringArray(ContentResolver.QUERY_ARG_SQL_SELECTION_ARGS, arguments)
    bundle.putString(ContentResolver.QUERY_ARG_SQL_SORT_ORDER, sort)

    if (limit != null && Build.VERSION.SDK_INT >= 30) {
        bundle.putInt(ContentResolver.QUERY_ARG_LIMIT, limit)
        bundle.putInt(ContentResolver.QUERY_ARG_OFFSET, offset)
    }

    // IContentProvider.query takes the caller differently on each of these
    // Android versions.
    val cursor = when {
        Build.VERSION.SDK_INT >= 31 -> {
            val caller = AttributionSource.Builder(Process.myUid()).setPackageName(SHELL).build()

            query.invoke(media, caller, uri, COLUMNS, bundle, null)
        }
        Build.VERSION.SDK_INT == 30 -> query.invoke(media, SHELL, null, uri, COLUMNS, bundle, null)
        Build.VERSION.SDK_INT >= 26 -> query.invoke(media, SHELL, uri, COLUMNS, bundle, null)
        else -> query.invoke(media, SHELL, uri, COLUMNS, selection, arguments, sort, null)
    }

    return cursor as Cursor? ?: throw IOException("media query failed")
}

fun page(offset: Int, limit: Int, folders: List<String>): List<Record> = gallery(folders, limit, offset).use { cursor ->
    // MediaStore takes a limit and offset only from Android 11. Before that
    // the query returns the whole gallery, so only the requested rows are
    // read.
    if (Build.VERSION.SDK_INT < 30 && offset > 0 && !cursor.moveToPosition(offset - 1)) {
        return emptyList()
    }

    val records = mutableListOf<Record>()

    while (records.size < limit && cursor.moveToNext()) {
        val record = listOf(
            cursor.getString(0).orEmpty(),
            cursor.getLong(1).toString(),
            cursor.getLong(2).toString(),
            cursor.getInt(3).toString(),
            cursor.getLong(4).toString(),
        )

        records.add(record.map { it.toByteArray() })
    }

    records
}
