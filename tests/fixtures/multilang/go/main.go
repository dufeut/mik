package main

import (
	"net/http"

	"github.com/rajatjindal/wasi-go-sdk/pkg/wasihttp"
)

func init() {
	wasihttp.Handle(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		w.Write([]byte(`{"message":"Hello from Go!","lang":"go"}`))
	})
}

func main() {}
