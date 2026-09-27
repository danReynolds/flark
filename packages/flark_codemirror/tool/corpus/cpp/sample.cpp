// geometry.cpp: shapes, templates and a small registry.
#pragma once
#include <iostream>
#include <map>
#include <memory>
#include <string>
#include <vector>

namespace geo {
namespace detail {

constexpr double kPi = 3.141'592'653;
constexpr unsigned long long kMask = 0xFFFF'FFFFull;
constexpr int kFlags = 0b1010'0101;

template <typename T>
T clamp(T value, T lo, T hi) {
  return value < lo ? lo : (value > hi ? hi : value);
}

}  // namespace detail

/**
 * The base of every shape.
 */
class Shape {
 public:
  explicit Shape(std::string name) : name_(std::move(name)) {}
  virtual ~Shape() = default;
  virtual double area() const noexcept = 0;
  const std::string& name() const { return name_; }

 protected:
  std::string name_;
};

class Circle final : public Shape {
 public:
  Circle(double radius);
  double area() const noexcept override;

 private:
  double radius_ = .5;
};

Circle::Circle(double radius)
    : Shape("circle"), radius_(detail::clamp(radius, 0.0, 1e6)) {}

double Circle::area() const noexcept {
  return detail::kPi * radius_ * radius_;
}

template <class Key, class Value = std::vector<Key>>
struct Registry {
  std::map<Key, Value> entries;

  void add(const Key& key, Value value) {
    entries.emplace(key, std::move(value));
  }
};

enum class Color : unsigned char { Red, Green = 4, Blue };

}  // namespace geo

static const char* kBanner = R"banner(
  Shapes "and" raw strings
  keep \n and quotes )" verbatim
)banner";

auto make_shapes() {
  std::vector<std::unique_ptr<geo::Shape>> shapes;
  shapes.push_back(std::make_unique<geo::Circle>(2.0f));
  return shapes;
}

int main(int argc, char* argv[]) {
  auto shapes = make_shapes();
  const wchar_t* wide = L"wide";
  const char8_t* utf8 = u8"utf-8 text";
  char32_t letter = U'x';
  const char16_t* utf16 = u"sixteen";
  auto raw = uR"(a raw "utf-16" string)";
  int* ptr = nullptr;
  bool ok = ptr == NULL || true;
  double total = 0;
  for (const auto& shape : shapes) {
    total += shape->area();
  }
  auto square = [&total](double x) -> double { return x * x + total; };
  auto [first, second] = std::pair<int, char>{1, 'c'};
  switch (static_cast<geo::Color>(argc)) {
    case geo::Color::Red:
      std::cout << "red" << std::endl;
      break;
    default: {
      std::cerr << "other: " << argc << '\n';
    }
  }
  try {
    if (argc > 3) throw std::runtime_error("too many");
  } catch (const std::exception& e) {
    std::cerr << e.what();
  }
  [[maybe_unused]] int unused = sizeof(int) * __CHAR_BIT__;
  using Map = std::map<std::string, int>;
  Map counts{{"a", 1}, {"b", 2}};
  std::cout << kBanner << wide << utf8 << letter << utf16 << raw
            << square(total) << first << second << ok << counts.size()
            << std::endl;
  return 0;
}
