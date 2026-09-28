# frozen_string_literal: true

require "json"
require_relative "support/money"

=begin
A small library catalog: books, loans and a report.
Nothing here is loaded by the tests.
=end

module Library
  VERSION = "2.3.1"
  GENRES = %w[fiction poetry history science].freeze
  STATES = %i[available loaned lost]

  class Error < StandardError; end
  class NotFound < Error
    def initialize(isbn)
      super("no book with ISBN #{isbn}")
    end
  end

  Book = Struct.new(:isbn, :title, :author, :year, keyword_init: true) do
    def to_s = "#{title} (#{year}) by #{author}"

    def classic?
      year < 1950
    end
  end

  class Catalog
    include Enumerable
    attr_reader :books, :loans

    @@instances = 0
    DEFAULT_DAYS = 14

    def initialize(books = [])
      @books = books.each_with_object({}) { |book, index| index[book.isbn] = book }
      @loans = Hash.new { |hash, key| hash[key] = [] }
      @@instances += 1
    end

    def self.load(path)
      data = JSON.parse(File.read(path), symbolize_names: true)
      new(data.map { |attrs| Book.new(**attrs) })
    rescue Errno::ENOENT => e
      warn "missing catalog: #{e.message}"
      new
    end

    def each(&block)
      return enum_for(:each) unless block_given?
      @books.each_value(&block)
    end

    def find!(isbn)
      @books.fetch(isbn) { raise NotFound, isbn }
    end

    def lend(isbn, to:, days: DEFAULT_DAYS)
      book = find!(isbn)
      if loaned?(isbn)
        return false
      elsif days <= 0 || days > 60
        raise ArgumentError, "days out of range: #{days}"
      end
      @loans[isbn] << { member: to, due: Time.now + days * 86_400 }
      true
    end

    def loaned?(isbn) = !@loans[isbn].empty?

    def overdue(now = Time.now)
      select do |book|
        @loans[book.isbn].any? { |loan| loan[:due] < now }
      end
    end

    def by_decade
      group_by { |b| b.year / 10 * 10 }.sort.to_h
    end

    def search(query)
      pattern = /#{Regexp.escape(query)}/i
      find_all { |b| b.title =~ pattern || b.author.match?(pattern) }
    end

    def label_for(book)
      case book.year
      when ..1899 then :antique
      when 1900...1950 then :classic
      when 1950..1999
        :modern
      else
        :contemporary
      end
    end

    def report(io = $stdout)
      io.puts <<~TEXT
        Catalog report (v#{VERSION})
          books: #{count}, loaned: #{@loans.count { |_, l| l.any? }}
      TEXT
      each_slice(2).with_index(1) do |pair, page|
        io.printf("%-3d %s\n", page, pair.map(&:title).join(" | "))
      end
      io.puts %Q{genres: #{GENRES.join(", ")}}
      io.puts %q(literal #{not interpolated})
      io.puts 'single #{also literal}'
    end

    private

    def normalize(isbn)
      isbn.to_s.delete("-").strip.upcase
    end
  end
end

catalog = Library::Catalog.new([
  Library::Book.new(isbn: "0-14-044913-6", title: "Walden", author: "Thoreau", year: 1854),
  Library::Book.new(isbn: "0-679-73452-9", title: "Invisible Cities", author: "Calvino", year: 1972),
])

begin
  catalog.lend("0-14-044913-6", to: "ada")
  catalog.lend("missing", to: "bob")
rescue Library::NotFound => error
  puts "error: #{error.message}"
ensure
  puts "loans: #{catalog.loans.keys.inspect}"
end

unless catalog.none?(&:classic?)
  puts catalog.map(&:to_s).sort_by { -_1.length }.first
end
counts = catalog.sum { |b| b.year > 1900 ? 1 : 0 }
puts "modern: #{counts}, first letter: #{?W}, pid: #$$"
