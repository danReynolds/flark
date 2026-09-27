#!/usr/bin/env ruby
# Task runner odds and ends: symbols, percent literals, globals, heredocs.

$verbose = ARGV.include?("-v")
STDOUT.sync = true

def log(msg) = $verbose && warn("[#{Time.now.strftime('%H:%M:%S')}] #{msg}")

OPERATORS = [:+, :-, :*, :/, :<=>, :==, :[], :<<, :!, :"quoted sym", :'single sym']
HEX, BIN, OCT, SCI = 0x1F, 0b1010, 0o17, 6.02e23
money = 1_000_000.50
ratio = 0.5
chars = [?a, ?\n, ?\C-a, ?\M-x]

tasks = {
  build: -> { system("make", "-j#{Etc.nprocessors}") },
  "clean" => proc { FileUtils.rm_rf(%w(tmp dist)) },
  lint: lambda do |files = Dir["**/*.rb"]|
    files.reject { |f| f.start_with?("vendor/") }.each { |f| yield f if block_given? }
  end,
}

commands = %x(git log --oneline -n 3).lines.map(&:chomp)
pattern = %r{^(\w+)/(\d+)$}
words = %W[alpha #{money} gamma]
angle = %w<one two three>
symbols = %s(dynamic)
braced = %{interpolated #{words.first} and {nested} braces}

if "feature/42" =~ pattern
  puts "branch #$1 number #{$2.to_i}"
end
puts "last error: #$!" if $!
puts "ivar in string: #@count and cvar #@@total" rescue nil

sql = <<-SQL
  SELECT *
  FROM tasks
  WHERE done = false
  SQL

template = <<~'RAW'
  No #{interpolation} happens here.
RAW

x = 10
x += 1 while x < 15
x -= 1 until x.even?
y = x > 12 ? "big" : "small"
z = x <=> 12
flag = !x.nil? && (x & 1).zero? || x ** 2 > 100
safe = tasks[:build]&.call
x ||= 3
list = [1, 2, 3].map { _1 * 2 }.select(&:positive?)
div = 10 / 2 / 5
path = File.join(__dir__, "tasks", "#{x}.rb")

class Runner
  attr_accessor :name

  def initialize(name) = @name = name

  def run(task, *args, **opts, &block)
    log "running #{task} with #{args.size} args"
    result = catch(:halt) do
      loop do
        break yield(task) if block
        throw :halt, :no_block
      end
    end
    for i in 1..3 do
      next if i == 2
      redo if false
    end
    result
  rescue StandardError => e
    retry if (opts[:attempts] -= 1).positive?
    raise
  end

  protected def secret = :hidden

  def method_missing(name, *args)
    name.to_s.end_with?("!") ? super : "#{name}?"
  end
end

Runner.new("ci").run(:build) { |t| puts "doing #{t}" }

# A def inside %{#{...}} leaves the mode's paused-quote context behind, whose
# indentation upstream computes as NaN until the next `end`.
named = %{defined #{def helper = 1}}
[1].each do |n|
  puts named * n
end
exit(0) unless defined?(Rake)
__END__
data after end
