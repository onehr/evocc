/* Local AES-GCM library demo; fixed public test key/nonce, never production. */
#include <psa/crypto.h>
#include <stdio.h>
#include <string.h>
int main(void)
{
    unsigned char key[16]={0}, nonce[12]={0}, plain[256], cipher[272], decoded[256];
    for(unsigned i=0;i<256;++i) plain[i]=(i/16==i%16 || i/16+i%16==15)?255:24;
    psa_key_attributes_t attr=PSA_KEY_ATTRIBUTES_INIT;
    mbedtls_svc_key_id_t id=0;
    size_t n=0,m=0;
    if(psa_crypto_init()!=PSA_SUCCESS) return 1;
    psa_set_key_usage_flags(&attr,PSA_KEY_USAGE_ENCRYPT|PSA_KEY_USAGE_DECRYPT);
    psa_set_key_algorithm(&attr,PSA_ALG_GCM);
    psa_set_key_type(&attr,PSA_KEY_TYPE_AES);
    psa_set_key_bits(&attr,128);
    if(psa_import_key(&attr,key,sizeof key,&id)!=PSA_SUCCESS) return 2;
    if(psa_aead_encrypt(id,PSA_ALG_GCM,nonce,sizeof nonce,NULL,0,plain,sizeof plain,
                        cipher,sizeof cipher,&n)!=PSA_SUCCESS) return 3;
    if(psa_aead_decrypt(id,PSA_ALG_GCM,nonce,sizeof nonce,NULL,0,cipher,n,
                        decoded,sizeof decoded,&m)!=PSA_SUCCESS) return 4;
    if(m!=sizeof plain || memcmp(plain,decoded,m)) return 5;
    printf("{\"plain\":\"");
    for(unsigned i=0;i<256;++i) printf("%02x",plain[i]);
    printf("\",\"cipher\":\"");
    for(size_t i=0;i<n;++i) printf("%02x",cipher[i]);
    cipher[0]^=1;
    psa_status_t result=psa_aead_decrypt(id,PSA_ALG_GCM,nonce,sizeof nonce,NULL,0,
                                       cipher,n,decoded,sizeof decoded,&m);
    if(result!=PSA_ERROR_INVALID_SIGNATURE) return 6;
    puts("\",\"restored\":true,\"tamperRejected\":true}");
    return psa_destroy_key(id)==PSA_SUCCESS?0:7;
}
