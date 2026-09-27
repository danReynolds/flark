@file:JvmName("Tasks")
package com.example.tasks

import kotlin.math.max
import kotlinx.coroutines.*

/**
 * A unit of work. /* Nested comments */ are allowed in Kotlin.
 */
data class Task(
    val id: Int,
    val title: String,
    val priority: Priority = Priority.NORMAL,
    val tags: List<String> = emptyList(),
)

enum class Priority(val weight: Int) {
    LOW(1), NORMAL(5), HIGH(10);

    fun isUrgent() = weight >= 10
}

sealed interface Result<out T> {
    data class Ok<T>(val value: T) : Result<T>
    data class Failed(val error: Throwable) : Result<Nothing>
    object Pending : Result<Nothing>
}

const val MAX_TASKS = 1_000
private val HEX = 0xFF_EC_DE_5E
private val MASK = 0b1101_0010L
private val RATIO = 2.5e-3f
private val UNSIGNED = 42u
private val half = .5

class TaskQueue<T : Comparable<T>>(private val capacity: Int = MAX_TASKS) {
    private val items = mutableListOf<T>()
    var size: Int = 0
        private set

    companion object {
        @JvmStatic
        fun <T : Comparable<T>> of(vararg values: T): TaskQueue<T> =
            TaskQueue<T>().apply { values.forEach(::add) }
    }

    fun add(item: T): Boolean {
        if (items.size >= capacity) return false
        items += item
        size = items.size
        return true
    }

    operator fun get(index: Int): T? = items.getOrNull(index)

    inline fun <reified R> filterIsInstance(): List<R> =
        items.filterIsInstance<R>()
}

fun describe(task: Task): String = when (task.priority) {
    Priority.HIGH -> "urgent: ${task.title.uppercase()}"
    Priority.NORMAL, Priority.LOW -> "task #${task.id} $task"
    else -> "unknown"
}

fun summary(tasks: List<Task>): String {
    val total = tasks
        .filter { it.priority != Priority.LOW }
        .sumOf { it.priority.weight }
    val longest = tasks.maxByOrNull { it.title.length }?.title ?: "none"
    val report = """
        |Tasks: ${tasks.size}
        |Total weight: $total
        |Longest: "$longest"
    """.trimMargin()
    val initial: Char = 'T'
    val escaped = "tab\t, quote \", dollar \$ and backslash \\"
    return report + initial + escaped
}

suspend fun process(queue: TaskQueue<Task>): Result<Int> = coroutineScope {
    var processed = 0
    outer@ for (i in 0 until queue.size step 2) {
        val task = queue[i] ?: continue
        for (tag in task.tags) {
            if (tag.isBlank()) continue@outer
            if (tag == "stop") break@outer
        }
        processed++
    }
    try {
        val deferred = async { delay(10L); processed * 2 }
        Result.Ok(deferred.await())
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        Result.Failed(e)
    } finally {
        println("done")
    }
}

fun main(args: Array<String>) {
    val queue = TaskQueue.of(
        Task(1, "write"),
        Task(2, "review", Priority.HIGH, listOf("code", "docs")),
    )
    val numbers = intArrayOf(1, 2, 3)
    val spread = listOf(*numbers.toTypedArray())
    val lengths = args.map { arg -> arg.length }.filter { it > 0 }
    val lazyValue: String by lazy { "computed" }
    val maxLength = lengths.fold(0) { acc, n ->
        max(acc, n)
    }
    runBlocking {
        when (val result = process(queue)) {
            is Result.Ok -> println("processed ${result.value}")
            is Result.Failed -> println(result.error)
            Result.Pending -> {}
        }
    }
    println(spread.size + maxLength + lazyValue.length + half)
}
