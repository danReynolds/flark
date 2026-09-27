// Package pipeline fans work out to a pool of workers and gathers results.
package pipeline

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"sync"
	"time"
)

const (
	defaultWorkers = 4
	maxRetries     = 3
	mask           = 0xFF &^ 0x0F
	permissions    = 0755
	ratio          = 1.5e-3
	half           = .5
	complexUnit    = 2i
)

type Stage int

const (
	Parse Stage = iota
	Validate
	Store
)

var ErrClosed = errors.New("pipeline: closed")

// Job is one unit of work; tags drive the JSON encoding.
type Job struct {
	ID       int               `json:"id"`
	Payload  string            `json:"payload,omitempty"`
	Attempts int               `json:"-"`
	Meta     map[string]string `json:"meta"`
}

type Result struct {
	Job   Job
	Value any
	Err   error
}

// Handler turns a job into a value.
type Handler interface {
	Handle(ctx context.Context, job Job) (any, error)
	Name() string
}

type HandlerFunc func(context.Context, Job) (any, error)

func (f HandlerFunc) Handle(ctx context.Context, job Job) (any, error) { return f(ctx, job) }
func (f HandlerFunc) Name() string                                      { return "func" }

type Pool[T comparable] struct {
	mu      sync.Mutex
	seen    map[T]bool
	workers int
	jobs    chan Job
	results chan<- Result
}

func NewPool[T comparable](workers int, out chan<- Result) *Pool[T] {
	if workers <= 0 {
		workers = defaultWorkers
	}
	return &Pool[T]{seen: make(map[T]bool), workers: workers, jobs: make(chan Job, workers*2), results: out}
}

func (p *Pool[T]) Run(ctx context.Context, h Handler) error {
	var wg sync.WaitGroup
	for i := 0; i < p.workers; i++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			for {
				select {
				case job, ok := <-p.jobs:
					if !ok {
						return
					}
					value, err := p.attempt(ctx, h, job)
					p.results <- Result{Job: job, Value: value, Err: err}
				case <-ctx.Done():
					return
				default:
					time.Sleep(10 * time.Millisecond)
				}
			}
		}(i)
	}
	wg.Wait()
	return ctx.Err()
}

func (p *Pool[T]) attempt(ctx context.Context, h Handler, job Job) (v any, err error) {
	defer func() {
		if r := recover(); r != nil {
			err = fmt.Errorf("handler %s panicked: %v", h.Name(), r)
		}
	}()
retry:
	for job.Attempts < maxRetries {
		job.Attempts++
		v, err = h.Handle(ctx, job)
		switch {
		case err == nil:
			return v, nil
		case errors.Is(err, ErrClosed):
			break retry
		case job.Attempts == maxRetries-1:
			fallthrough
		default:
			continue
		}
	}
	return nil, err
}

func describe(s Stage, r rune) string {
	var b strings.Builder
	switch s {
	case Parse, Validate:
		b.WriteString("early")
	case Store:
		b.WriteString("late")
	default:
		panic("unknown stage")
	}
	if r == '\'' || r == '\n' || r == '\x00' {
		b.WriteRune('?')
	}
	b.WriteString(` raw \n stays`)
	return b.String() + "\t\"done\"" + string(len("héllo"))
}

const π, café = 3.14159, "crème"

const banner = `pipeline v1
	multi-line raw string with "quotes" and \ backslashes
end of banner`

/* A block comment
   spanning lines, with * stars and // slashes. */
var escapes = []string{"\u00e9", "\x41", "\101", "\\"}
