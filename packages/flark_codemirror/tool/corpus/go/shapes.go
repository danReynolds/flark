package main

import "fmt"
import m "math"

type Point struct{ X, Y float64 }

type Shape interface {
	Area() float64
	Perimeter() float64
}

type Circle struct {
	Point
	Radius float64
}

type Rect struct {
	Min, Max Point
	Label    *string
}

func (c Circle) Area() float64      { return m.Pi * c.Radius * c.Radius }
func (c Circle) Perimeter() float64 { return 2 * m.Pi * c.Radius }

func (r Rect) Area() float64 {
	w, h := r.Max.X-r.Min.X, r.Max.Y-r.Min.Y
	return w * h
}

func (r Rect) Perimeter() float64 {
	return 2*(r.Max.X-r.Min.X) + 2*(r.Max.Y-r.Min.Y)
}

func classify(v interface{}) string {
	switch x := v.(type) {
	case nil:
		return "nil"
	case int, int64:
		return fmt.Sprintf("integer %d", x)
	case string:
		return "string of " + fmt.Sprint(len(x))
	case Shape:
		return fmt.Sprintf("shape with area %.2f", x.Area())
	}
	return "unknown"
}

func Sum[N int | float64](xs ...N) (total N) {
	for _, x := range xs {
		total += x
	}
	return
}

func main() {
	label := "unit"
	shapes := []Shape{
		Circle{Point{0, 0}, 1},
		Rect{Min: Point{X: 1, Y: 1}, Max: Point{3, 4}, Label: &label},
	}
	byKind := map[string][]Shape{}
	for i, s := range shapes {
		kind := fmt.Sprintf("%T", s)
		byKind[kind] = append(byKind[kind], s)
		fmt.Printf("%d: %s %v\n", i, classify(s), s)
	}

	done := make(chan struct{})
	counts := make(chan int)
	go func() {
		defer close(counts)
		for n := 1; n <= 3; n++ {
			counts <- n * n
		}
	}()
	go func() {
		for c := range counts {
			fmt.Println("square", c)
		}
		done <- struct{}{}
	}()
	<-done

	anon := struct {
		Name string
		Tags []string
	}{"demo", []string{"a", "b"}}
	fmt.Println(anon.Name, len(anon.Tags), cap(anon.Tags))

	i := 0
loop:
	if i < 3 {
		i++
		goto loop
	}
	var total = Sum(1.5, 2.5, 3.0)
	x, y := 10, 3
	x <<= 2
	y |= x & 0x0f
	ok := x > y && !(y == 0) || x%2 != 0
	fmt.Println(total, x, y, ok, x/y, -x, ^y)
	if err := recover(); err != nil {
		panic(err)
	}
	select {}
}
